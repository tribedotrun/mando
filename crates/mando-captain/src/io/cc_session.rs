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
//!   - Two concurrent `POST /api/ops/start` calls with the same `key` could
//!     both observe "no session" and launch two CC runs; the second writer
//!     would overwrite the first, leaking the first run's session id and
//!     breaking `/api/ops/end`.
//!   - A concurrent `follow_up` or `start` during `cleanup_expired` could
//!     reinsert the key between the expiry snapshot and the remove, causing
//!     cleanup to delete a live session. `cleanup_expired` now holds the
//!     sync map lock through the entire check-and-remove for each key.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;
use tracing::{info, warn};

use mando_cc::{CcConfig, CcOneShot, CcResult};
use mando_types::now_rfc3339;

/// Build a completed (Stopped) session log entry from a CcResult.
fn make_session_entry<'a>(
    result: &'a CcResult,
    cwd: &'a Path,
    model: &'a str,
    caller: &'a str,
    task_id: &'a str,
    resumed: bool,
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
        status: mando_types::SessionStatus::Stopped,
        worker_name: "",
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
    pub async fn start(
        &self,
        key: &str,
        prompt: &str,
        cwd: &Path,
        model: Option<&str>,
        idle_ttl: Duration,
        call_timeout: Duration,
    ) -> Result<CcResult> {
        self.start_with_item(key, prompt, cwd, model, idle_ttl, call_timeout, "")
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
        task_id: &str,
    ) -> Result<CcResult> {
        // Serialize same-key starts. Different keys run in parallel.
        let key_mu = self.key_lock(key);
        let _key_guard = key_mu.lock().await;
        self.start_locked(key, prompt, cwd, model, idle_ttl, call_timeout, task_id)
            .await
    }

    /// Atomic replace-and-start: close any existing session for `key`, then
    /// start a fresh one — all under the per-key async mutex. Use this for
    /// handlers like `POST /api/ops/start` where the intent is "clobber
    /// whatever's there and begin from scratch".
    #[allow(clippy::too_many_arguments)]
    pub async fn start_replacing(
        &self,
        key: &str,
        prompt: &str,
        cwd: &Path,
        model: Option<&str>,
        idle_ttl: Duration,
        call_timeout: Duration,
    ) -> Result<CcResult> {
        let key_mu = self.key_lock(key);
        let _key_guard = key_mu.lock().await;
        // Synchronously close any existing session (remove from map + delete
        // file). The per-key lock prevents a concurrent start / follow_up
        // from observing a half-closed state.
        self.close(key);
        self.start_locked(key, prompt, cwd, model, idle_ttl, call_timeout, "")
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
        task_id: &str,
    ) -> Result<CcResult> {
        let result = CcOneShot::run(
            prompt,
            CcConfig::builder()
                .model(model.unwrap_or(&self.default_model))
                .cwd(cwd)
                .timeout(call_timeout)
                .caller(key)
                .task_id(task_id)
                .build(),
        )
        .await?;

        crate::io::headless_cc::log_cc_session(
            &self.pool,
            &make_session_entry(
                &result,
                cwd,
                model.unwrap_or(&self.default_model),
                key,
                task_id,
                false,
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
    pub async fn follow_up(&self, key: &str, message: &str, cwd: &Path) -> Result<CcResult> {
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

        let result = CcOneShot::run(
            message,
            CcConfig::builder()
                .model(&self.default_model)
                .cwd(cwd)
                .timeout(Duration::from_secs(session.call_timeout_s))
                .caller(key)
                .resume(session.session_id.clone())
                .build(),
        )
        .await?;

        crate::io::headless_cc::log_cc_session(
            &self.pool,
            &make_session_entry(&result, cwd, &self.default_model, key, "", true),
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

    /// Check if a session exists and is not expired.
    pub fn has_session(&self, key: &str) -> bool {
        use time::format_description::well_known::Rfc3339;
        let sessions = self.sessions_lock();
        sessions.get(key).is_some_and(|s| {
            time::OffsetDateTime::parse(&s.last_active, &Rfc3339)
                .map_err(|e| {
                    tracing::error!(module = "cc-session", key = %key, error = %e, "unparseable last_active on persisted session; treating as expired")
                })
                .ok()
                .is_some_and(|t| {
                    let elapsed = (time::OffsetDateTime::now_utc() - t).as_seconds_f64() as u64;
                    elapsed < s.idle_ttl_s
                })
        })
    }

    /// Remove all expired sessions (idle beyond their TTL).
    ///
    /// Re-checks expiry under the sync `sessions` lock for each key
    /// immediately before removal, so a session refreshed by a concurrent
    /// `follow_up` between scan and remove cannot be deleted.
    pub fn cleanup_expired(&self) -> usize {
        use time::format_description::well_known::Rfc3339;

        fn is_expired(s: &CcSession) -> bool {
            time::OffsetDateTime::parse(&s.last_active, &Rfc3339)
                .ok()
                .is_none_or(|t| {
                    let elapsed = (time::OffsetDateTime::now_utc() - t).as_seconds_f64() as u64;
                    elapsed >= s.idle_ttl_s
                })
        }

        // Phase 1: drain expired entries from the map under a single lock.
        // Holding the lock across the iterate+remove keeps the removal
        // atomic with the expiry recheck — no concurrent refresh can slip
        // in between the check and the remove.
        let removed: Vec<(String, CcSession)> = {
            let mut sessions = self.sessions_lock();
            let keys: Vec<String> = sessions
                .iter()
                .filter(|(_, s)| is_expired(s))
                .map(|(k, _)| k.clone())
                .collect();
            keys.into_iter()
                .filter_map(|k| {
                    // Re-check under the same lock — a concurrent
                    // `follow_up` holding the per-key async mutex could
                    // NOT have refreshed yet (it would need the sync lock
                    // to write), so this check is authoritative.
                    match sessions.get(&k) {
                        Some(s) if is_expired(s) => sessions.remove(&k).map(|s| (k, s)),
                        _ => None,
                    }
                })
                .collect()
        };

        let count = removed.len();
        // Phase 2: best-effort file cleanup for the entries we actually
        // removed. Safe to release the sync lock for I/O: the key has
        // been removed from the map, so any concurrent caller with the
        // same key either sees "no session" (start will create a new
        // file after we finish deleting the old one — correct) or holds
        // the per-key async mutex (their write to `sessions` and persist
        // will happen after us).
        for (key, _) in &removed {
            let path = self.session_path(key);
            if let Err(e) = std::fs::remove_file(&path) {
                if e.kind() != std::io::ErrorKind::NotFound {
                    warn!(module = "cc-session", key = %key, error = %e, "failed to remove expired session file");
                }
            }
            info!(module = "cc-session", key = %key, "expired session cleaned up");
        }

        // Also prune idle per-key mutexes that no one is holding — their
        // only reason to exist is to serialize live callers. Safe because
        // any future caller will recreate the mutex on demand.
        {
            let mut locks = self
                .key_locks
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            locks.retain(|_, mu| Arc::strong_count(mu) > 1);
        }

        count
    }

    /// Recover sessions from disk on restart. Returns recovered count and
    /// corrupt-file count so callers can surface both.
    pub fn recover(&self) -> RecoverStats {
        let dir = &self.state_dir;
        if !dir.is_dir() {
            return RecoverStats::default();
        }
        let mut stats = RecoverStats::default();
        match std::fs::read_dir(dir) {
            Ok(entries) => {
                for entry in entries {
                    let entry = match entry {
                        Ok(e) => e,
                        Err(e) => {
                            warn!(module = "cc-session", error = %e, "failed to read directory entry during recovery");
                            stats.corrupt += 1;
                            continue;
                        }
                    };
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) != Some("json") {
                        continue;
                    }
                    let data = match std::fs::read_to_string(&path) {
                        Ok(d) => d,
                        Err(e) => {
                            warn!(module = "cc-session", path = %path.display(), error = %e, "failed to read session file during recovery");
                            stats.corrupt += 1;
                            continue;
                        }
                    };
                    match serde_json::from_str::<CcSession>(&data) {
                        Ok(session) => {
                            let key = path
                                .file_stem()
                                .and_then(|s| s.to_str())
                                .unwrap_or("")
                                .to_string();
                            if !key.is_empty() {
                                let mut sessions = self.sessions_lock();
                                sessions.insert(key, session);
                                stats.recovered += 1;
                            }
                        }
                        Err(e) => {
                            warn!(module = "cc-session", path = %path.display(), error = %e, "corrupt session file during recovery");
                            stats.corrupt += 1;
                        }
                    }
                }
            }
            Err(e) => {
                warn!(module = "cc-session", dir = %dir.display(), error = %e, "failed to read session directory during recovery");
            }
        }
        if stats.recovered > 0 || stats.corrupt > 0 {
            info!(
                module = "cc-session",
                recovered = stats.recovered,
                corrupt = stats.corrupt,
                "recovered sessions from disk"
            );
        }
        stats
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

impl Drop for CcSessionManager {
    fn drop(&mut self) {
        let count = self.sessions.get_mut().map(|m| m.len()).unwrap_or(0);
        if count > 0 {
            warn!(
                module = "cc-session",
                count, "dropping manager with active sessions"
            );
        }
    }
}
