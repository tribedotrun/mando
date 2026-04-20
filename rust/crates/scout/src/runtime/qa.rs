//! Scout Q&A — persistent multi-turn CC sessions.
//!
//! Instead of spawning a new process per turn (with `--resume` overhead),
//! keeps a single CC process alive per Q&A session. Follow-up questions
//! are sent via stdin; answers arrive on stdout. The session manager
//! handles lifecycle, TTL expiry, and cleanup.
//!
//! Per-session locking: the outer map mutex is only held briefly for
//! lookup/insert/remove — never across async I/O. Each session has its
//! own `Arc<Mutex<LiveSession>>` so concurrent requests to different
//! sessions don't block each other.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use settings::config::ScoutWorkflow;
use tokio::sync::Mutex;
use tracing::{info, warn};

/// Result of a Q&A invocation.
pub struct QaResult {
    pub answer: String,
    pub session_id: Option<String>,
    pub suggested_followups: Vec<String>,
    /// True when the requested session was not found and a fresh one was created.
    pub session_reset: bool,
    /// Total cost in USD for this turn.
    pub cost_usd: Option<f64>,
    /// Duration in milliseconds for this turn.
    pub duration_ms: Option<u64>,
    /// Credential used for this turn (if any).
    pub credential_id: Option<i64>,
}

/// A live Q&A session holding a persistent CC process.
///
/// `cc` is `Option` so we can `take()` it out for `close()` (which consumes self).
struct LiveSession {
    cc: Option<global_claude::CcSession>,
    session_id: String,
    last_active: Instant,
}

/// Manages persistent Q&A CC sessions with TTL-based expiry.
pub struct QaSessionManager {
    sessions: Mutex<HashMap<String, Arc<Mutex<LiveSession>>>>,
    ttl: Duration,
    qa_timeout: Duration,
}

impl QaSessionManager {
    pub fn new(ttl: Duration, qa_timeout: Duration) -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            ttl,
            qa_timeout,
        }
    }

    /// Ask a question — creates a new session or reuses an existing one.
    #[allow(clippy::too_many_arguments)]
    #[tracing::instrument(skip_all)]
    pub async fn ask(
        &self,
        question: &str,
        summary: &str,
        article: &str,
        raw_content_note: Option<&str>,
        workflow: &ScoutWorkflow,
        session_key: Option<&str>,
        pool: &sqlx::SqlitePool,
    ) -> Result<QaResult> {
        self.expire_stale().await;

        let credential = settings::credentials::pick_for_worker(pool, None)
            .await
            .inspect_err(|e| warn!(error = %e, "scout-qa: pick_for_worker failed"))
            .unwrap_or(None);

        // Try to reuse an existing session.
        if let Some(key) = session_key {
            // Brief map lock — grab the Arc, then release.
            let session_arc = { self.sessions.lock().await.get(key).cloned() };

            if let Some(session_arc) = session_arc {
                match self.try_live_followup(&session_arc, key, question).await {
                    Ok(r) => return Ok(r),
                    Err(reason) => {
                        warn!(module = "scout-qa", key = %key, %reason, "live follow-up failed, falling back to resume");
                        return self
                            .ask_via_resume(question, key, workflow, &credential)
                            .await
                            .map_err(|e| e.context(format!("{reason}; resume fallback failed")));
                    }
                }
            }

            warn!(module = "scout-qa", key = %key, "live session missing, trying resume");
            match self
                .ask_via_resume(question, key, workflow, &credential)
                .await
            {
                Ok(result) => return Ok(result),
                Err(e) => {
                    warn!(module = "scout-qa", key = %key, error = %e, "resume failed, creating fresh session");
                }
            }
        }

        // Create new session with full article context.
        let prompt =
            render_first_turn_prompt(question, summary, article, raw_content_note, workflow)?;

        let mut cc =
            global_claude::CcSession::spawn(qa_cc_config(workflow, None, &credential)?).await?;
        let session_id = cc.session_id().to_string();

        cc.send_message(&prompt).await?;

        let timeout = workflow.agent.qa_timeout_s;
        let result = match tokio::time::timeout(timeout, cc.recv_result()).await {
            Ok(Ok(r)) => r,
            Ok(Err(e)) => {
                if let Err(ce) = cc.close().await {
                    warn!(module = "scout-qa", error = %ce, "close failed on first-turn error");
                }
                return Err(anyhow::Error::from(e).context("Q&A first turn failed"));
            }
            Err(_) => {
                if let Err(ce) = cc.close().await {
                    warn!(module = "scout-qa", error = %ce, "close failed on first-turn timeout");
                }
                anyhow::bail!("Q&A first turn timed out after {timeout:?}");
            }
        };

        let keep_live = global_claude::is_process_alive(cc.pid());
        let mut qa_result = parse_qa_result(&result, &session_id);
        // Use result session_id (CC may reassign) for the map key.
        let key = qa_result
            .session_id
            .clone()
            .unwrap_or_else(|| session_id.clone());
        qa_result.session_reset = session_key.is_some();
        qa_result.credential_id = global_claude::credential_id(&credential);

        if keep_live {
            self.sessions.lock().await.insert(
                key,
                Arc::new(Mutex::new(LiveSession {
                    cc: Some(cc),
                    session_id,
                    last_active: Instant::now(),
                })),
            );
        } else {
            info!(module = "scout-qa", session_id = %session_id, "Q&A process exited after first turn; future follow-ups will use resume");
        }

        Ok(qa_result)
    }

    /// Attempt a follow-up on a live session. Returns the QaResult on
    /// success, or a reason string on failure (session already removed from map).
    async fn try_live_followup(
        &self,
        session_arc: &Arc<Mutex<LiveSession>>,
        key: &str,
        question: &str,
    ) -> Result<QaResult, String> {
        let mut live = session_arc.lock().await;
        live.last_active = Instant::now();
        let session_id = live.session_id.clone();

        let Some(cc) = live.cc.as_mut() else {
            drop(live);
            return self
                .drop_and_remove(key, "live session already closed".into())
                .await;
        };

        if let Err(e) = cc.send_message(question).await {
            drop(live);
            return self.drop_and_remove(key, format!("send failed: {e}")).await;
        }

        let result = match tokio::time::timeout(self.qa_timeout, cc.recv_result()).await {
            Ok(Ok(r)) => r,
            Ok(Err(e)) => {
                drop(live);
                return self.drop_and_remove(key, format!("recv failed: {e}")).await;
            }
            Err(_) => {
                drop(live);
                return self
                    .drop_and_remove(key, format!("recv timed out after {:?}", self.qa_timeout))
                    .await;
            }
        };

        let keep_live = global_claude::is_process_alive(cc.pid());
        let mut r = parse_qa_result(&result, &session_id);
        r.session_reset = false;
        drop(live);
        if !keep_live {
            info!(module = "scout-qa", %session_id, "Q&A process exited after response; future follow-ups will use resume");
            self.sessions.lock().await.remove(key);
        }
        Ok(r)
    }

    /// Remove a session from the map and return an error reason. Used by
    /// `try_live_followup` when any step (send/recv/timeout) fails — the live
    /// session is abandoned and the next call falls back to resume.
    async fn drop_and_remove(&self, key: &str, reason: String) -> Result<QaResult, String> {
        self.sessions.lock().await.remove(key);
        Err(reason)
    }

    async fn ask_via_resume(
        &self,
        question: &str,
        session_key: &str,
        workflow: &ScoutWorkflow,
        credential: &Option<(i64, String)>,
    ) -> Result<QaResult> {
        let result = global_claude::CcOneShot::run(
            question,
            qa_cc_config(workflow, Some(session_key), credential)?,
        )
        .await
        .with_context(|| format!("resume Q&A session {session_key}"))?;

        let mut qa_result = parse_qa_result(&result, session_key);
        qa_result.credential_id = global_claude::credential_id(credential);
        qa_result.session_reset = false;
        Ok(qa_result)
    }

    /// Close a specific session.
    #[tracing::instrument(skip_all)]
    pub async fn close(&self, session_key: &str) {
        let removed = self.sessions.lock().await.remove(session_key);
        if let Some(arc) = removed {
            let mut live = arc.lock().await;
            info!(module = "scout-qa", session_id = %live.session_id, "closing Q&A session");
            if let Some(cc) = live.cc.take() {
                if let Err(e) = cc.close().await {
                    warn!(module = "scout-qa", error = %e, "failed to close CC process");
                }
            }
        }
    }

    /// Expire sessions inactive longer than TTL. Closes outside the map lock.
    async fn expire_stale(&self) {
        let stale: Vec<Arc<Mutex<LiveSession>>> = {
            let mut sessions = self.sessions.lock().await;
            let now = Instant::now();
            let keys: Vec<String> = sessions
                .iter()
                .filter_map(|(k, arc)| {
                    arc.try_lock()
                        .ok()
                        .filter(|s| now.duration_since(s.last_active) > self.ttl)
                        .map(|_| k.clone())
                })
                .collect();
            keys.into_iter()
                .filter_map(|k| sessions.remove(&k))
                .collect()
        };
        close_sessions(stale, "expiring stale").await;
    }

    /// Shut down all active sessions (for graceful shutdown). Closes outside map lock.
    #[tracing::instrument(skip_all)]
    pub async fn shutdown(&self) {
        let all: Vec<Arc<Mutex<LiveSession>>> =
            { self.sessions.lock().await.drain().map(|(_, v)| v).collect() };
        close_sessions(all, "shutting down").await;
    }
}

/// Close a set of live sessions, logging progress and any errors. Called from
/// both `expire_stale` and `shutdown` after they've removed the sessions from
/// the map.
async fn close_sessions(arcs: Vec<Arc<Mutex<LiveSession>>>, reason: &str) {
    for arc in arcs {
        let mut live = arc.lock().await;
        info!(module = "scout-qa", session_id = %live.session_id, reason, "closing Q&A session");
        if let Some(cc) = live.cc.take() {
            if let Err(e) = cc.close().await {
                warn!(module = "scout-qa", error = %e, reason, "failed to close CC process");
            }
        }
    }
}

/// Build the session manager from workflow config.
pub fn session_manager_from_workflow(workflow: &ScoutWorkflow) -> Arc<QaSessionManager> {
    Arc::new(QaSessionManager::new(
        workflow.agent.qa_ttl_s,
        workflow.agent.qa_timeout_s,
    ))
}

fn qa_cc_config(
    workflow: &ScoutWorkflow,
    resume_session: Option<&str>,
    credential: &Option<(i64, String)>,
) -> anyhow::Result<global_claude::CcConfig> {
    let model = crate::service::model_lookup::required_model(workflow, "qa")?;
    let mut builder = global_claude::CcConfig::builder()
        .model(model)
        .timeout(workflow.agent.qa_timeout_s)
        .caller("scout-qa")
        .json_schema(qa_json_schema());
    if let Some(session_id) = resume_session {
        builder = builder.resume(session_id);
    }
    builder = global_claude::with_credential(builder, credential);
    Ok(builder.build())
}

// ---------------------------------------------------------------------------
// Internal
// ---------------------------------------------------------------------------

use super::qa_parse::{parse_qa_result, qa_json_schema, render_first_turn_prompt};

#[cfg(all(test, feature = "dev-mocks"))]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn rust_workspace_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf()
    }

    fn cc_mock_binary() -> String {
        let rust_workspace_root = rust_workspace_root();
        let debug = rust_workspace_root.join("target/debug/mando-cc-mock");
        if debug.exists() {
            return debug.to_string_lossy().into_owned();
        }
        let release = rust_workspace_root.join("target/release/mando-cc-mock");
        if release.exists() {
            return release.to_string_lossy().into_owned();
        }

        let status = std::process::Command::new("cargo")
            .args(["build", "-p", "dev-cc-mock", "--bin", "mando-cc-mock"])
            .current_dir(&rust_workspace_root)
            .status()
            .expect("failed to invoke cargo build for dev-cc-mock");
        assert!(
            status.success(),
            "cargo build -p dev-cc-mock failed with status {status}"
        );
        assert!(
            debug.exists(),
            "mando-cc-mock still missing after cargo build"
        );
        debug.to_string_lossy().into_owned()
    }

    #[tokio::test]
    async fn follow_up_can_resume_after_live_session_is_closed() {
        let _lock = global_infra::PROCESS_ENV_LOCK.lock().await;
        let temp = std::env::temp_dir().join(format!(
            "mando-scout-qa-test-{}",
            global_infra::uuid::Uuid::v4()
        ));
        std::fs::create_dir_all(&temp).unwrap();
        let _bin_guard = global_infra::EnvVarGuard::set("MANDO_CC_CLAUDE_BIN", cc_mock_binary());
        let _data_guard = global_infra::EnvVarGuard::set("MANDO_DATA_DIR", &temp);

        let workflow = ScoutWorkflow::compiled_default();
        let mgr = QaSessionManager::new(Duration::from_secs(600), Duration::from_secs(120));
        let db = global_db::Db::open_in_memory().await.unwrap();
        let pool = db.pool();

        let first = mgr
            .ask(
                "What is this?",
                "Short summary",
                "Article body",
                None,
                &workflow,
                None,
                pool,
            )
            .await
            .unwrap();
        let session_id = first.session_id.clone().unwrap();
        assert!(!first.answer.is_empty());

        mgr.close(&session_id).await;

        let second = mgr
            .ask(
                "Is it useful?",
                "Short summary",
                "Article body",
                None,
                &workflow,
                Some(&session_id),
                pool,
            )
            .await
            .unwrap();
        assert_eq!(second.session_id.as_deref(), Some(session_id.as_str()));
        assert!(!second.answer.is_empty());
        assert!(!second.session_reset);

        let _ = std::fs::remove_dir_all(&temp);
    }
}
