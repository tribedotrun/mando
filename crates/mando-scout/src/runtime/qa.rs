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
use mando_config::workflow::ScoutWorkflow;
use tokio::sync::Mutex;
use tracing::{info, warn};

/// Result of a Q&A invocation.
pub struct QaResult {
    pub answer: String,
    pub session_id: Option<String>,
    pub suggested_followups: Vec<String>,
    /// True when the requested session was not found and a fresh one was created.
    pub session_reset: bool,
}

/// JSON schema for structured Q&A responses.
fn qa_json_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "answer": { "type": "string" },
            "suggested_followups": {
                "type": "array",
                "items": { "type": "string" }
            }
        },
        "required": ["answer", "suggested_followups"]
    })
}

/// A live Q&A session holding a persistent CC process.
///
/// `cc` is `Option` so we can `take()` it out for `close()` (which consumes self).
struct LiveSession {
    cc: Option<mando_cc::CcSession>,
    session_id: String,
    last_active: Instant,
}

/// Manages persistent Q&A CC sessions with TTL-based expiry.
pub struct QaSessionManager {
    sessions: Mutex<HashMap<String, Arc<Mutex<LiveSession>>>>,
    ttl: Duration,
}

impl QaSessionManager {
    pub fn new(ttl: Duration) -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            ttl,
        }
    }

    /// Ask a question — creates a new session or reuses an existing one.
    pub async fn ask(
        &self,
        question: &str,
        summary: &str,
        article: &str,
        raw_content_note: Option<&str>,
        workflow: &ScoutWorkflow,
        session_key: Option<&str>,
    ) -> Result<QaResult> {
        self.expire_stale().await;

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
                            .ask_via_resume(question, key, workflow)
                            .await
                            .map_err(|e| e.context(format!("{reason}; resume fallback failed")));
                    }
                }
            }

            warn!(module = "scout-qa", key = %key, "live session missing, trying resume");
            match self.ask_via_resume(question, key, workflow).await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    warn!(module = "scout-qa", key = %key, error = %e, "resume failed, creating fresh session");
                }
            }
        }

        // Create new session with full article context.
        let prompt =
            render_first_turn_prompt(question, summary, article, raw_content_note, workflow)?;

        let mut cc = mando_cc::CcSession::spawn(qa_cc_config(workflow, None)?).await?;
        let session_id = cc.session_id().to_string();

        cc.send_message(&prompt).await?;

        let timeout = Duration::from_secs(120);
        let result = match tokio::time::timeout(timeout, cc.recv_result()).await {
            Ok(Ok(r)) => r,
            Ok(Err(e)) => {
                if let Err(ce) = cc.close().await {
                    warn!(module = "scout-qa", error = %ce, "close failed on first-turn error");
                }
                return Err(e.context("Q&A first turn failed"));
            }
            Err(_) => {
                if let Err(ce) = cc.close().await {
                    warn!(module = "scout-qa", error = %ce, "close failed on first-turn timeout");
                }
                anyhow::bail!("Q&A first turn timed out after {timeout:?}");
            }
        };

        let keep_live = mando_cc::is_process_alive(cc.pid());
        let mut qa_result = parse_qa_result(&result, &session_id);
        // Use result session_id (CC may reassign) for the map key.
        let key = qa_result
            .session_id
            .clone()
            .unwrap_or_else(|| session_id.clone());
        qa_result.session_reset = session_key.is_some();

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

        let timeout = Duration::from_secs(120);
        let result = match tokio::time::timeout(timeout, cc.recv_result()).await {
            Ok(Ok(r)) => r,
            Ok(Err(e)) => {
                drop(live);
                return self.drop_and_remove(key, format!("recv failed: {e}")).await;
            }
            Err(_) => {
                drop(live);
                return self
                    .drop_and_remove(key, format!("recv timed out after {timeout:?}"))
                    .await;
            }
        };

        let keep_live = mando_cc::is_process_alive(cc.pid());
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
    ) -> Result<QaResult> {
        let result = mando_cc::CcOneShot::run(question, qa_cc_config(workflow, Some(session_key))?)
            .await
            .with_context(|| format!("resume Q&A session {session_key}"))?;

        let mut qa_result = parse_qa_result(&result, session_key);
        qa_result.session_reset = false;
        Ok(qa_result)
    }

    /// Close a specific session.
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

/// Build the default session manager (10 min TTL).
pub fn default_session_manager() -> Arc<QaSessionManager> {
    Arc::new(QaSessionManager::new(Duration::from_secs(600)))
}

fn qa_cc_config(
    workflow: &ScoutWorkflow,
    resume_session: Option<&str>,
) -> anyhow::Result<mando_cc::CcConfig> {
    let model = crate::biz::model_lookup::required_model(workflow, "qa")?;
    let mut builder = mando_cc::CcConfig::builder()
        .model(model)
        .timeout(Duration::from_secs(120))
        .caller("scout-qa")
        .json_schema(qa_json_schema());
    if let Some(session_id) = resume_session {
        builder = builder.resume(session_id);
    }
    Ok(builder.build())
}

// ---------------------------------------------------------------------------
// Internal
// ---------------------------------------------------------------------------

fn render_first_turn_prompt(
    question: &str,
    summary: &str,
    article: &str,
    raw_content_note: Option<&str>,
    workflow: &ScoutWorkflow,
) -> anyhow::Result<String> {
    let raw_note = raw_content_note.unwrap_or("");
    let user_context_rendered = workflow.user_context.render();

    let mut vars: rustc_hash::FxHashMap<&str, &str> = rustc_hash::FxHashMap::default();
    vars.insert("question", question);
    vars.insert("summary", summary);
    vars.insert("article", article);
    vars.insert("raw_content_note", raw_note);
    vars.insert("user_context", user_context_rendered.as_str());

    mando_config::render_prompt("qa", &workflow.prompts, &vars).map_err(|e| anyhow::anyhow!(e))
}

fn extract_followups(val: &serde_json::Value) -> Vec<String> {
    val["suggested_followups"]
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

fn parse_qa_result(result: &mando_cc::CcResult, ctx_sid: &str) -> QaResult {
    let make = |answer: String, followups: Vec<String>| QaResult {
        answer,
        session_id: Some(result.session_id.clone()),
        suggested_followups: followups,
        session_reset: false,
    };

    if let Some(ref structured) = result.structured {
        let answer = structured["answer"]
            .as_str()
            .map(String::from)
            .unwrap_or_else(|| {
                warn!(module = "scout-qa", session_id = %ctx_sid, "structured output has no 'answer', falling back to text");
                result.text.clone()
            });
        return make(answer, extract_followups(structured));
    }

    warn!(module = "scout-qa", session_id = %ctx_sid, "no structured output, trying text JSON extraction");
    let parsed = match mando_captain::biz::json_parse::parse_llm_json(&result.text) {
        Ok(v) => v,
        Err(e) => {
            warn!(module = "scout-qa", error = %e, "JSON extraction failed, using raw text");
            return make(result.text.clone(), Vec::new());
        }
    };
    if let Some(answer) = parsed["answer"].as_str() {
        return make(answer.to_string(), extract_followups(&parsed));
    }

    warn!(module = "scout-qa", session_id = %ctx_sid, "JSON extraction failed, using raw text as answer");
    make(result.text.clone(), Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn workspace_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf()
    }

    fn cc_mock_binary() -> String {
        let workspace_root = workspace_root();
        let debug = workspace_root.join("target/debug/mando-cc-mock");
        if debug.exists() {
            return debug.to_string_lossy().into_owned();
        }
        let release = workspace_root.join("target/release/mando-cc-mock");
        if release.exists() {
            return release.to_string_lossy().into_owned();
        }

        let status = std::process::Command::new("cargo")
            .args(["build", "-p", "dev-cc-mock", "--bin", "mando-cc-mock"])
            .current_dir(&workspace_root)
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
        let temp =
            std::env::temp_dir().join(format!("mando-scout-qa-test-{}", mando_uuid::Uuid::v4()));
        std::fs::create_dir_all(&temp).unwrap();
        let old_bin = std::env::var("MANDO_CC_CLAUDE_BIN").ok();
        let old_data = std::env::var("MANDO_DATA_DIR").ok();
        std::env::set_var("MANDO_CC_CLAUDE_BIN", cc_mock_binary());
        std::env::set_var("MANDO_DATA_DIR", &temp);

        let workflow = ScoutWorkflow::compiled_default();
        let mgr = QaSessionManager::new(Duration::from_secs(600));

        let first = mgr
            .ask(
                "What is this?",
                "Short summary",
                "Article body",
                None,
                &workflow,
                None,
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
            )
            .await
            .unwrap();
        assert_eq!(second.session_id.as_deref(), Some(session_id.as_str()));
        assert!(!second.answer.is_empty());
        assert!(!second.session_reset);

        if let Some(val) = old_bin {
            std::env::set_var("MANDO_CC_CLAUDE_BIN", val);
        } else {
            std::env::remove_var("MANDO_CC_CLAUDE_BIN");
        }
        if let Some(val) = old_data {
            std::env::set_var("MANDO_DATA_DIR", val);
        } else {
            std::env::remove_var("MANDO_DATA_DIR");
        }
        let _ = std::fs::remove_dir_all(&temp);
    }
}
