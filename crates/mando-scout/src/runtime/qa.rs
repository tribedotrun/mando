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

use anyhow::Result;
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
                let mut live = session_arc.lock().await;
                live.last_active = Instant::now();
                let session_id = live.session_id.clone();

                let cc = live
                    .cc
                    .as_mut()
                    .ok_or_else(|| anyhow::anyhow!("Q&A session {session_id} CC already closed"))?;

                if let Err(e) = cc.send_message(question).await {
                    warn!(module = "scout-qa", %session_id, error = %e, "send failed, removing broken session");
                    drop(live);
                    self.sessions.lock().await.remove(key);
                    return Err(e.context("Q&A follow-up send failed"));
                }

                let timeout = Duration::from_secs(120);
                let result = match tokio::time::timeout(timeout, cc.recv_result()).await {
                    Ok(Ok(r)) => r,
                    Ok(Err(e)) => {
                        warn!(module = "scout-qa", %session_id, error = %e, "recv failed, removing session");
                        drop(live);
                        self.sessions.lock().await.remove(key);
                        return Err(e.context("Q&A follow-up failed"));
                    }
                    Err(_) => {
                        warn!(module = "scout-qa", %session_id, "recv timed out, removing session");
                        drop(live);
                        self.sessions.lock().await.remove(key);
                        anyhow::bail!("Q&A follow-up timed out after {timeout:?}");
                    }
                };

                let mut r = parse_qa_result(&result, &session_id);
                r.session_reset = false;
                return Ok(r);
            }

            warn!(module = "scout-qa", key = %key, "session expired, creating new");
        }

        // Create new session with full article context.
        let prompt =
            render_first_turn_prompt(question, summary, article, raw_content_note, workflow)?;

        let model = workflow.models.get("qa").cloned().unwrap_or_else(|| {
            tracing::warn!(
                module = "scout",
                "missing 'qa' model in workflow config, using empty default"
            );
            String::new()
        });
        let config = mando_cc::CcConfig::builder()
            .model(model)
            .timeout(Duration::from_secs(120))
            .caller("scout-qa")
            .json_schema(qa_json_schema())
            .build();

        let mut cc = mando_cc::CcSession::spawn(config).await?;
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

        let mut qa_result = parse_qa_result(&result, &session_id);
        // Use result session_id (CC may reassign) for the map key.
        let key = qa_result
            .session_id
            .clone()
            .unwrap_or_else(|| session_id.clone());
        qa_result.session_reset = session_key.is_some();

        self.sessions.lock().await.insert(
            key,
            Arc::new(Mutex::new(LiveSession {
                cc: Some(cc),
                session_id,
                last_active: Instant::now(),
            })),
        );

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

        for arc in stale {
            let mut live = arc.lock().await;
            info!(module = "scout-qa", session_id = %live.session_id, "expiring stale Q&A session");
            if let Some(cc) = live.cc.take() {
                if let Err(e) = cc.close().await {
                    warn!(module = "scout-qa", error = %e, "failed to close expired CC process");
                }
            }
        }
    }

    /// Shut down all active sessions (for graceful shutdown). Closes outside map lock.
    pub async fn shutdown(&self) {
        let all: Vec<Arc<Mutex<LiveSession>>> =
            { self.sessions.lock().await.drain().map(|(_, v)| v).collect() };
        for arc in all {
            let mut live = arc.lock().await;
            info!(module = "scout-qa", session_id = %live.session_id, "shutting down Q&A session");
            if let Some(cc) = live.cc.take() {
                if let Err(e) = cc.close().await {
                    warn!(module = "scout-qa", error = %e, "failed to close CC on shutdown");
                }
            }
        }
    }
}

/// Build the default session manager (10 min TTL).
pub fn default_session_manager() -> Arc<QaSessionManager> {
    Arc::new(QaSessionManager::new(Duration::from_secs(600)))
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

    let mut vars = std::collections::HashMap::new();
    vars.insert("question", question);
    vars.insert("summary", summary);
    vars.insert("article", article);
    vars.insert("raw_content_note", raw_note);
    vars.insert("user_context", user_context_rendered.as_str());

    mando_config::render_prompt("qa", &workflow.prompts, &vars).map_err(|e| anyhow::anyhow!(e))
}

fn parse_qa_result(result: &mando_cc::CcResult, ctx_sid: &str) -> QaResult {
    if let Some(ref structured) = result.structured {
        let answer = match structured["answer"].as_str() {
            Some(a) => a.to_string(),
            None => {
                warn!(module = "scout-qa", session_id = %ctx_sid, "structured output has no 'answer', falling back to text");
                result.text.clone()
            }
        };
        let followups = structured["suggested_followups"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        return QaResult {
            answer,
            session_id: Some(result.session_id.clone()),
            suggested_followups: followups,
            session_reset: false,
        };
    }

    warn!(module = "scout-qa", session_id = %ctx_sid, "no structured output, trying text JSON extraction");
    let parsed = mando_captain::biz::json_parse::parse_llm_json(&result.text);
    if let Some(answer) = parsed["answer"].as_str() {
        let followups = parsed["suggested_followups"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        return QaResult {
            answer: answer.to_string(),
            session_id: Some(result.session_id.clone()),
            suggested_followups: followups,
            session_reset: false,
        };
    }

    warn!(module = "scout-qa", session_id = %ctx_sid, "JSON extraction failed, using raw text as answer");
    QaResult {
        answer: result.text.clone(),
        session_id: Some(result.session_id.clone()),
        suggested_followups: Vec::new(),
        session_reset: false,
    }
}
