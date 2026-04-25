//! CC Session Manager — persistent multi-turn sessions via `claude --resume`.
//!
//! Used by: clarifier (B6), /ops (C1), /ask (C2).
//!
//! ## Locking
//!
//! Public methods take `&self`. Two independent locks cooperate:
//!
//! 1. `sessions` (sync `Mutex<HashMap>`) — protects the in-memory session
//!    map. Held only across synchronous HashMap / file operations; NEVER
//!    across `.await`.
//! 2. `key_locks` (sync `Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>`)
//!    — per-key async mutex that serializes `start_with_item`,
//!    `follow_up`, and `close` for a single key. Different keys proceed in
//!    parallel, preserving the goal of the interior-mutability refactor,
//!    while same-key operations are atomic end-to-end (including the long
//!    CC API call).
//!
//! The per-key mutex fixes two concurrency regressions introduced by the
//! move from `Arc<RwLock<CcSessionManager>>` to `Arc<CcSessionManager>`:
//!
//!   - Two concurrent `start_replacing` calls with the same `key` could
//!     both observe "no session" and launch two CC runs; the second writer
//!     would overwrite the first, leaking the first run's session id and
//!     breaking subsequent `close`.
//!   - A concurrent `follow_up` or `start` during `cleanup_expired` could
//!     reinsert the key between the expiry snapshot and the remove, causing
//!     cleanup to delete a live session. `cleanup_expired` now holds the
//!     sync map lock through the entire check-and-remove for each key.

mod lifecycle;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;
use tracing::{info, warn};

use global_claude::{CcConfig, CcResult};
use global_types::now_rfc3339;

/// Build a completed (Stopped) session log entry from a CcResult.
#[allow(clippy::too_many_arguments)]
fn make_session_entry<'a>(
    result: &'a CcResult<serde_json::Value>,
    cwd: &'a Path,
    model: &'a str,
    caller: &'a str,
    task_id: Option<i64>,
    resumed: bool,
    credential_id: Option<i64>,
) -> crate::io::headless_cc::SessionLogEntry<'a> {
    crate::io::headless_cc::SessionLogEntry {
        session_id: &result.session_id,
        cwd,
        model,
        caller,
        cost_usd: result.cost_usd,
        duration_ms: result.duration_ms,
        resumed,
        task_id,
        status: global_types::SessionStatus::Stopped,
        worker_name: "",
        credential_id,
        error: None,
        api_error_status: None,
    }
}

/// A persistent multi-turn CC session.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct CcSession {
    pub session_id: String,
    pub started_at: String,
    pub idle_ttl_s: u64,
    pub call_timeout_s: u64,
    pub last_active: String,
}

/// Summary of a CcSessionManager recover pass.
#[derive(Debug, Clone, Copy, Default)]
pub struct RecoverStats {
    pub recovered: usize,
    pub corrupt: usize,
}

/// Manages multiple named CC sessions with persistence.
pub struct CcSessionManager {
    sessions: Mutex<HashMap<String, CcSession>>,
    /// Per-key async mutexes. Serializes `start_with_item` / `follow_up` /
    /// `close` for a single key so concurrent same-key calls cannot race
    /// on the check-close-start sequence.
    key_locks: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    state_dir: PathBuf,
    default_model: String,
    pool: sqlx::SqlitePool,
}

impl CcSessionManager {
    pub fn new(state_dir: PathBuf, default_model: &str, pool: sqlx::SqlitePool) -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            key_locks: Mutex::new(HashMap::new()),
            state_dir,
            default_model: default_model.to_string(),
            pool,
        }
    }

    /// Briefly lock the sessions map. The caller MUST NOT `.await` while the
    /// returned guard is in scope.
    fn sessions_lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, CcSession>> {
        self.sessions
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Obtain the per-key async mutex, creating it on first use. The sync
    /// outer lock is held only briefly (one HashMap lookup / insert); the
    /// returned `Arc<tokio::sync::Mutex<()>>` is acquired by the caller
    /// across its `.await` points.
    fn key_lock(&self, key: &str) -> Arc<tokio::sync::Mutex<()>> {
        let mut map = self
            .key_locks
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        map.entry(key.to_string())
            .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone()
    }

    /// Start a new CC session. Returns the first response.
    #[allow(clippy::too_many_arguments)]
    pub async fn start(
        &self,
        key: &str,
        prompt: &str,
        cwd: &Path,
        model: Option<&str>,
        idle_ttl: Duration,
        call_timeout: Duration,
        max_turns: Option<u32>,
    ) -> Result<CcResult<serde_json::Value>> {
        self.start_with_item(
            key,
            prompt,
            cwd,
            model,
            idle_ttl,
            call_timeout,
            None,
            max_turns,
        )
        .await
    }

    /// Start a new CC session linked to a task.
    #[allow(clippy::too_many_arguments)]
    pub async fn start_with_item(
        &self,
        key: &str,
        prompt: &str,
        cwd: &Path,
        model: Option<&str>,
        idle_ttl: Duration,
        call_timeout: Duration,
        task_id: Option<i64>,
        max_turns: Option<u32>,
    ) -> Result<CcResult<serde_json::Value>> {
        // Serialize same-key starts. Different keys run in parallel.
        let key_mu = self.key_lock(key);
        let _key_guard = key_mu.lock().await;
        self.start_locked(
            key,
            prompt,
            cwd,
            model,
            idle_ttl,
            call_timeout,
            task_id,
            max_turns,
        )
        .await
    }

    /// Atomic replace-and-start: close any existing session for `key`, then
    /// start a fresh one — all under the per-key async mutex. Use this when
    /// the intent is "clobber whatever's there and begin from scratch".
    #[allow(clippy::too_many_arguments)]
    pub async fn start_replacing(
        &self,
        key: &str,
        prompt: &str,
        cwd: &Path,
        model: Option<&str>,
        idle_ttl: Duration,
        call_timeout: Duration,
    ) -> Result<CcResult<serde_json::Value>> {
        let key_mu = self.key_lock(key);
        let _key_guard = key_mu.lock().await;
        // Synchronously close any existing session (remove from map + delete
        // file). The per-key lock prevents a concurrent start / follow_up
        // from observing a half-closed state.
        self.close(key);
        self.start_locked(key, prompt, cwd, model, idle_ttl, call_timeout, None, None)
            .await
    }

    /// Body of `start_with_item` assuming the per-key lock is already held.
    #[allow(clippy::too_many_arguments)]
    async fn start_locked(
        &self,
        key: &str,
        prompt: &str,
        cwd: &Path,
        model: Option<&str>,
        idle_ttl: Duration,
        call_timeout: Duration,
        task_id: Option<i64>,
        max_turns: Option<u32>,
    ) -> Result<CcResult<serde_json::Value>> {
        let resolved_model = model.unwrap_or(&self.default_model).to_string();
        let model_ref = resolved_model.as_str();
        let cwd_ref = cwd;
        let key_ref = key;
        let task_id_copy = task_id;
        let max_turns_copy = max_turns;
        let result = settings::cc_failover::run_with_credential_failover(
            &self.pool,
            key_ref,
            prompt,
            |ctx| {
                let mut builder = CcConfig::builder()
                    .model(model_ref)
                    .cwd(cwd_ref.to_path_buf())
                    .timeout(call_timeout)
                    .caller(key_ref);
                if let Some(tid) = task_id_copy {
                    builder = builder.task_id(tid.to_string());
                }
                if let Some(n) = max_turns_copy {
                    builder = builder.max_turns(n);
                }
                builder = global_claude::with_credential(builder, &ctx.credential);
                if let Some(rid) = &ctx.resume_session_id {
                    builder = builder.resume(rid);
                }
                builder.build()
            },
        )
        .await?;
        let cred_id = result.credential_id;

        crate::io::headless_cc::log_cc_session(
            &self.pool,
            &make_session_entry(
                &result,
                cwd,
                model.unwrap_or(&self.default_model),
                key,
                task_id,
                false,
                cred_id,
            ),
        )
        .await?;

        let session_id = result.session_id.clone();

        let now = now_rfc3339();
        let session = CcSession {
            session_id: session_id.clone(),
            started_at: now.clone(),
            idle_ttl_s: idle_ttl.as_secs(),
            call_timeout_s: call_timeout.as_secs(),
            last_active: now,
        };

        {
            let mut sessions = self.sessions_lock();
            sessions.insert(key.to_string(), session.clone());
        }
        self.persist_session(key, &session)?;

        info!(module = "cc-session", key = %key, session_id = %session_id, "started session");
        Ok(result)
    }

    /// Follow up on an existing session via --resume.
    pub async fn follow_up(
        &self,
        key: &str,
        message: &str,
        cwd: &Path,
    ) -> Result<CcResult<serde_json::Value>> {
        // Serialize same-key follow-ups and starts; keeps the `last_active`
        // update race-free against concurrent starts and cleanup.
        let key_mu = self.key_lock(key);
        let _key_guard = key_mu.lock().await;

        let session = {
            let sessions = self.sessions_lock();
            sessions
                .get(key)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("no active session for '{}'", key))?
        };

        let cwd_ref = cwd;
        let key_ref = key;
        let session_sid = session.session_id.clone();
        let session_sid_ref = session_sid.as_str();
        let model_ref = self.default_model.as_str();
        let call_timeout = Duration::from_secs(session.call_timeout_s);
        let result = settings::cc_failover::run_with_credential_failover(
            &self.pool,
            key_ref,
            message,
            |ctx| {
                // Resume the key's anchor session unless the failover
                // wrapper overrides it with a later resume id (e.g. the
                // just-failed session after a 429 swap — which in this
                // path is the same id anyway since CC keeps the sid on
                // --resume).
                let resume_sid = ctx.resume_session_id.as_deref().unwrap_or(session_sid_ref);
                let mut builder = CcConfig::builder()
                    .model(model_ref)
                    .cwd(cwd_ref.to_path_buf())
                    .timeout(call_timeout)
                    .caller(key_ref)
                    .resume(resume_sid);
                builder = global_claude::with_credential(builder, &ctx.credential);
                builder.build()
            },
        )
        .await?;
        let cred_id = result.credential_id;

        crate::io::headless_cc::log_cc_session(
            &self.pool,
            &make_session_entry(&result, cwd, &self.default_model, key, None, true, cred_id),
        )
        .await?;

        // Update last_active.
        let updated = {
            let mut sessions = self.sessions_lock();
            sessions.get_mut(key).map(|s| {
                s.last_active = now_rfc3339();
                s.clone()
            })
        };
        if let Some(cloned) = updated {
            self.persist_session(key, &cloned)?;
        }

        Ok(result)
    }

    /// Close a session and remove from disk.
    ///
    /// Synchronous: does not acquire the per-key async mutex. Callers that
    /// need to serialize a close against a concurrent start/follow_up on
    /// the same key should perform both under a single-key critical section
    /// (or call `close_async` below, which does acquire the per-key lock).
    pub fn close(&self, key: &str) {
        let removed = {
            let mut sessions = self.sessions_lock();
            sessions.remove(key)
        };
        if removed.is_some() {
            let path = self.session_path(key);
            if let Err(e) = std::fs::remove_file(&path) {
                if e.kind() != std::io::ErrorKind::NotFound {
                    warn!(module = "cc-session", key = %key, error = %e, "failed to remove session file on close");
                }
            }
            info!(module = "cc-session", key = %key, "closed session");
        }
    }

    /// Async close that acquires the per-key lock so it cannot race with a
    /// concurrent start/follow_up on the same key.
    pub async fn close_async(&self, key: &str) {
        let key_mu = self.key_lock(key);
        let _key_guard = key_mu.lock().await;
        self.close(key);
    }

    fn session_path(&self, key: &str) -> PathBuf {
        self.state_dir.join(format!("{}.json", key))
    }

    fn persist_session(&self, key: &str, session: &CcSession) -> Result<()> {
        std::fs::create_dir_all(&self.state_dir)?;
        let path = self.session_path(key);
        let json = serde_json::to_string_pretty(session)?;
        std::fs::write(path, json)?;
        Ok(())
    }
}
