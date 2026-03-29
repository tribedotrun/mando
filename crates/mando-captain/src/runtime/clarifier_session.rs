use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;
use mando_config::workflow::CaptainWorkflow;
use mando_types::{now_rfc3339, Task};
use tracing::{info, warn};

use crate::io::cc_session::CcSessionManager;

use super::clarifier::{
    build_interactive_clarifier_turn_prompt, parse_clarifier_response, resolve_clarifier_cwd,
    ClarifierResult,
};

/// One turn in a clarifier conversation.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(super) struct ClarifierTurn {
    pub role: String,
    pub text: String,
    pub ts: String,
}

/// Persisted metadata for a stateful clarifier session.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(super) struct ClarifierSession {
    pub item_id: String,
    pub item_title: String,
    #[serde(default)]
    pub cwd: String,
    pub history: Vec<ClarifierTurn>,
    pub created_at: String,
    pub ttl_secs: u64,
}

const DEFAULT_TTL_SECS: u64 = 30 * 60; // 30 minutes
const CC_CALL_TIMEOUT_SECS: u64 = 90;

/// Manages multiple active multi-turn clarifier sessions.
pub struct ClarifierSessionManager {
    cc: CcSessionManager,
    sessions: HashMap<String, ClarifierSession>,
    state_dir: PathBuf,
}

impl ClarifierSessionManager {
    pub fn new(default_model: &str, pool: sqlx::SqlitePool) -> Self {
        let state_dir = mando_config::state_dir().join("clarifier_sessions");
        let cc_state_dir = state_dir.join("cc");
        Self {
            cc: CcSessionManager::new(cc_state_dir, default_model, pool),
            sessions: HashMap::new(),
            state_dir,
        }
    }

    pub fn set_default_model(&mut self, default_model: &str) {
        self.cc.set_default_model(default_model);
    }

    /// Start a new multi-turn clarifier session for an item.
    pub async fn start(
        &mut self,
        key: &str,
        item: &Task,
        human_input: &str,
        workflow: &CaptainWorkflow,
        config: &mando_config::Config,
    ) -> Result<ClarifierResult> {
        let prompt = build_interactive_clarifier_turn_prompt(item, workflow, Some(human_input))?;
        let cwd = resolve_clarifier_cwd(item, config);
        let ttl = Duration::from_secs(DEFAULT_TTL_SECS);
        let timeout = Duration::from_secs(CC_CALL_TIMEOUT_SECS);

        let item_id = item.best_id().to_string();
        let cc_result = self
            .cc
            .start_with_item(
                key,
                &prompt,
                &cwd,
                Some(&workflow.models.clarifier),
                ttl,
                timeout,
                &item_id,
            )
            .await?;

        let now = now_rfc3339();
        let mut history = vec![ClarifierTurn {
            role: "human".into(),
            text: human_input.to_string(),
            ts: now.clone(),
        }];

        let mut result = parse_clarifier_response(&cc_result.text, &item.title);
        result.session_id = Some(cc_result.session_id);

        if let Some(ref q) = result.questions {
            history.push(ClarifierTurn {
                role: "assistant".into(),
                text: q.clone(),
                ts: now_rfc3339(),
            });
        }

        let session = ClarifierSession {
            item_id: item.id.to_string(),
            item_title: item.title.clone(),
            cwd: cwd.display().to_string(),
            history,
            created_at: now,
            ttl_secs: DEFAULT_TTL_SECS,
        };

        self.sessions.insert(key.to_string(), session.clone());
        self.persist_session(key, &session)?;

        info!(
            module = "clarifier-session",
            key = %key,
            status = ?result.status,
            "session started"
        );
        Ok(result)
    }

    /// Continue an existing clarifier session with human input.
    pub async fn follow_up(&mut self, key: &str, human_input: &str) -> Result<ClarifierResult> {
        let item_title = self
            .sessions
            .get(key)
            .ok_or_else(|| anyhow::anyhow!("no clarifier session for '{}'", key))?
            .item_title
            .clone();

        let cwd = self
            .sessions
            .get(key)
            .map(|session| PathBuf::from(&session.cwd))
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| {
                let fallback = mando_config::state_dir();
                warn!(
                    module = "clarifier-session",
                    key = %key,
                    fallback = %fallback.display(),
                    "session has no cwd — falling back to state dir"
                );
                fallback
            });
        let cc_result = self.cc.follow_up(key, human_input, &cwd).await?;

        let now = now_rfc3339();
        let session = self
            .sessions
            .get_mut(key)
            .ok_or_else(|| anyhow::anyhow!("session vanished for '{}'", key))?;

        session.history.push(ClarifierTurn {
            role: "human".into(),
            text: human_input.to_string(),
            ts: now,
        });

        let mut result = parse_clarifier_response(&cc_result.text, &item_title);
        result.session_id = Some(cc_result.session_id);

        if let Some(ref q) = result.questions {
            session.history.push(ClarifierTurn {
                role: "assistant".into(),
                text: q.clone(),
                ts: now_rfc3339(),
            });
        }

        let cloned = session.clone();
        self.persist_session(key, &cloned)?;

        info!(
            module = "clarifier-session",
            key = %key,
            status = ?result.status,
            "session follow-up"
        );
        Ok(result)
    }

    /// Close and remove a session.
    pub fn close(&mut self, key: &str) {
        self.sessions.remove(key);
        self.cc.close(key);
        let path = self.state_dir.join(format!("{key}.json"));
        if let Err(e) = std::fs::remove_file(&path) {
            if path.exists() {
                warn!(
                    module = "clarifier-session",
                    key = %key,
                    path = %path.display(),
                    error = %e,
                    "failed to delete session file — may resurrect on restart"
                );
            }
        }
        info!(module = "clarifier-session", key = %key, "session closed");
    }

    /// Check if a session exists and is not expired.
    pub fn has_session(&self, key: &str) -> bool {
        self.cc.has_session(key)
    }

    /// Recover sessions from disk after restart.
    pub fn recover(&mut self) -> usize {
        let cc_count = self.cc.recover();
        if !self.state_dir.is_dir() {
            return cc_count;
        }
        let mut count = 0;
        if let Ok(entries) = std::fs::read_dir(&self.state_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if !path.is_file() {
                    continue;
                }
                if path.extension().and_then(|e| e.to_str()) != Some("json") {
                    continue;
                }
                match std::fs::read_to_string(&path) {
                    Ok(data) => match serde_json::from_str::<ClarifierSession>(&data) {
                        Ok(session) => {
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
                        Err(e) => {
                            warn!(
                                module = "clarifier-session",
                                path = %path.display(),
                                error = %e,
                                "skipping corrupt session file"
                            );
                        }
                    },
                    Err(e) => {
                        warn!(
                            module = "clarifier-session",
                            path = %path.display(),
                            error = %e,
                            "failed to read session file"
                        );
                    }
                }
            }
        }
        if count > 0 {
            info!(
                module = "clarifier-session",
                count = count,
                "recovered sessions from disk"
            );
        }
        count
    }

    fn persist_session(&self, key: &str, session: &ClarifierSession) -> Result<()> {
        std::fs::create_dir_all(&self.state_dir)?;
        let path = self.state_dir.join(format!("{key}.json"));
        let json = serde_json::to_string_pretty(session)?;
        std::fs::write(path, json)?;
        Ok(())
    }
}
