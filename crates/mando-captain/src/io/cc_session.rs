//! CC Session Manager — persistent multi-turn sessions via `claude --resume`.
//!
//! Used by: clarifier (B6), /ops (C1), /ask (C2).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Result;
use tracing::{info, warn};

use mando_cc::{CcConfig, CcOneShot, CcResult};

/// A persistent multi-turn CC session.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct CcSession {
    pub session_id: String,
    pub started_at: String,
    pub idle_ttl_s: u64,
    pub call_timeout_s: u64,
    pub last_active: String,
}

/// Manages multiple named CC sessions with persistence.
pub struct CcSessionManager {
    sessions: HashMap<String, CcSession>,
    state_dir: PathBuf,
    default_model: String,
    pool: sqlx::SqlitePool,
}

impl CcSessionManager {
    pub fn new(state_dir: PathBuf, default_model: &str, pool: sqlx::SqlitePool) -> Self {
        Self {
            sessions: HashMap::new(),
            state_dir,
            default_model: default_model.to_string(),
            pool,
        }
    }

    /// Start a new CC session. Returns the first response.
    pub async fn start(
        &mut self,
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
        &mut self,
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
            &crate::io::headless_cc::SessionLogEntry {
                session_id: &result.session_id,
                cwd,
                model: model.unwrap_or(&self.default_model),
                caller: key,
                cost_usd: result.cost_usd,
                duration_ms: result.duration_ms,
                resumed: false,
                task_id,
                status: mando_types::SessionStatus::Stopped,
                worker_name: "",
            },
        )
        .await;

        let session_id = result.session_id.clone();

        let now = now_rfc3339();
        let session = CcSession {
            session_id: session_id.clone(),
            started_at: now.clone(),
            idle_ttl_s: idle_ttl.as_secs(),
            call_timeout_s: call_timeout.as_secs(),
            last_active: now,
        };

        self.sessions.insert(key.to_string(), session.clone());
        self.persist_session(key, &session)?;

        info!(module = "cc-session", key = %key, session_id = %session_id, "started session");
        Ok(result)
    }

    /// Follow up on an existing session via --resume.
    pub async fn follow_up(&mut self, key: &str, message: &str, cwd: &Path) -> Result<CcResult> {
        let session = self
            .sessions
            .get(key)
            .ok_or_else(|| anyhow::anyhow!("no active session for '{}'", key))?
            .clone();

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
            &crate::io::headless_cc::SessionLogEntry {
                session_id: &result.session_id,
                cwd,
                model: &self.default_model,
                caller: key,
                cost_usd: result.cost_usd,
                duration_ms: result.duration_ms,
                resumed: true,
                task_id: "",
                status: mando_types::SessionStatus::Stopped,
                worker_name: "",
            },
        )
        .await;

        // Update last_active.
        if let Some(s) = self.sessions.get_mut(key) {
            s.last_active = now_rfc3339();
            let cloned = s.clone();
            self.persist_session(key, &cloned)?;
        }

        Ok(result)
    }

    /// Close a session and remove from disk.
    pub fn close(&mut self, key: &str) {
        if self.sessions.remove(key).is_some() {
            let path = self.session_path(key);
            std::fs::remove_file(&path).ok();
            info!(module = "cc-session", key = %key, "closed session");
        }
    }

    /// Check if a session exists and is not expired.
    pub fn has_session(&self, key: &str) -> bool {
        use time::format_description::well_known::Rfc3339;
        self.sessions.get(key).is_some_and(|s| {
            time::OffsetDateTime::parse(&s.last_active, &Rfc3339)
                .map_err(|e| {
                    tracing::warn!(module = "cc-session", key = %key, error = %e, "unparseable last_active")
                })
                .ok()
                .is_some_and(|t| {
                    let elapsed = (time::OffsetDateTime::now_utc() - t).as_seconds_f64() as u64;
                    elapsed < s.idle_ttl_s
                })
        })
    }

    /// Remove all expired sessions (idle beyond their TTL).
    pub fn cleanup_expired(&mut self) -> usize {
        let expired_keys: Vec<String> = self
            .sessions
            .keys()
            .filter(|k| !self.has_session(k))
            .cloned()
            .collect();
        let count = expired_keys.len();
        for key in &expired_keys {
            let path = self.session_path(key);
            std::fs::remove_file(&path).ok();
            self.sessions.remove(key);
            info!(module = "cc-session", key = %key, "expired session cleaned up");
        }
        count
    }

    /// Recover sessions from disk on restart.
    pub fn recover(&mut self) -> usize {
        let dir = &self.state_dir;
        if !dir.is_dir() {
            return 0;
        }
        let mut count = 0;
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) != Some("json") {
                    continue;
                }
                if let Ok(data) = std::fs::read_to_string(&path) {
                    if let Ok(session) = serde_json::from_str::<CcSession>(&data) {
                        let key = path
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("")
                            .to_string();
                        if !key.is_empty() {
                            self.sessions.insert(key, session);
                            count += 1;
                        }
                    }
                }
            }
        }
        if count > 0 {
            info!(
                module = "cc-session",
                count = count,
                "recovered sessions from disk"
            );
        }
        count
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

use mando_types::now_rfc3339;

impl Drop for CcSessionManager {
    fn drop(&mut self) {
        if !self.sessions.is_empty() {
            warn!(
                module = "cc-session",
                count = self.sessions.len(),
                "dropping manager with active sessions"
            );
        }
    }
}
