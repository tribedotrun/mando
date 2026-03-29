//! Session management methods for the Telegram bot.
//!
//! Input, ops, ask, QA, and act sessions — extracted from bot.rs for file length.

use anyhow::Result;
use tracing::{debug, warn};

use mando_shared::{escape_html, render_markdown_reply_html};

use crate::bot::{ActSession, QaSession, Session, TelegramBot};
use crate::gateway_paths as paths;

impl TelegramBot {
    // ── Input sessions ───────────────────────────────────────────────

    pub fn has_input_session(&self, cid: &str) -> bool {
        self.input_sessions.contains_key(cid)
    }
    pub fn input_session_title(&self, cid: &str) -> Option<String> {
        self.input_sessions.get(cid).cloned()
    }
    pub fn open_input_session(&mut self, cid: &str, title: &str) {
        self.input_sessions
            .insert(cid.to_string(), title.to_string());
    }
    pub fn close_input_session(&mut self, cid: &str) {
        self.input_sessions.remove(cid);
    }

    /// Close local input session AND cancel the server-side clarifier session.
    pub async fn close_input_session_with_cancel(&mut self, cid: &str) {
        self.input_sessions.remove(cid);
        // Fire-and-forget cancel — ignore errors (session may not exist server-side).
        let gw = self.gw.clone();
        let key = cid.to_string();
        tokio::spawn(async move {
            gw.post("/api/clarifier/cancel", &serde_json::json!({"key": key}))
                .await
                .ok();
        });
    }

    // ── Ops sessions ─────────────────────────────────────────────────

    pub fn has_ops_session(&self, cid: &str) -> bool {
        self.ops_sessions.contains_key(cid)
    }
    pub fn ops_session_rounds(&self, cid: &str) -> u32 {
        self.ops_sessions.get(cid).map(|s| s.rounds).unwrap_or(0)
    }
    pub fn open_ops_session(&mut self, cid: &str) {
        self.ops_sessions
            .insert(cid.to_string(), Session::default());
    }
    pub fn close_ops_session(&mut self, cid: &str) {
        self.ops_sessions.remove(cid);
    }
    pub fn increment_ops_rounds(&mut self, cid: &str) {
        if let Some(s) = self.ops_sessions.get_mut(cid) {
            s.rounds += 1;
        }
    }

    // ── Ask sessions ─────────────────────────────────────────────────

    pub fn has_ask_session(&self, cid: &str) -> bool {
        self.ask_sessions.contains_key(cid)
    }
    pub fn ask_session_rounds(&self, cid: &str) -> u32 {
        self.ask_sessions.get(cid).map(|s| s.rounds).unwrap_or(0)
    }
    pub fn open_ask_session(&mut self, cid: &str) {
        // Close conflicting scout QA session so task-ask wins plain-text routing
        self.qa_sessions.remove(cid);
        self.ask_sessions
            .insert(cid.to_string(), Session::default());
    }
    pub fn close_ask_session(&mut self, cid: &str) {
        self.ask_sessions.remove(cid);
    }
    pub fn increment_ask_rounds(&mut self, cid: &str) {
        if let Some(s) = self.ask_sessions.get_mut(cid) {
            s.rounds += 1;
        }
    }

    // ── Scout QA sessions ───────────────────────────────────────────

    pub fn open_qa_session(&mut self, cid: &str, item_id: i64) {
        // Close conflicting task-ask session so scout QA wins plain-text routing
        self.ask_sessions.remove(cid);
        self.qa_sessions.insert(
            cid.to_string(),
            QaSession {
                item_id,
                rounds: 0,
                cc_session_id: None,
            },
        );
    }

    pub fn close_qa_session(&mut self, cid: &str) {
        self.qa_sessions.remove(cid);
    }

    pub(crate) async fn handle_qa_text(&mut self, chat_id: &str, question: &str) -> Result<()> {
        let (item_id, cc_session_id) = match self.qa_sessions.get(chat_id) {
            Some(s) => (s.item_id, s.cc_session_id.clone()),
            None => return Ok(()),
        };

        let ack = self
            .api
            .send_message(chat_id, "\u{1f4ac} Thinking\u{2026}", None, None, true)
            .await?;
        let ack_mid = ack.get("message_id").and_then(|v| v.as_i64()).unwrap_or(0);

        let body = serde_json::json!({
            "id": item_id,
            "question": question,
            "session_id": cc_session_id,
        });
        let result = self.gw.post(paths::SCOUT_ASK, &body).await;

        let answer = match result {
            Ok(ref resp) => {
                if let Some(sid) = resp["session_id"].as_str() {
                    if let Some(session) = self.qa_sessions.get_mut(chat_id) {
                        session.cc_session_id = Some(sid.to_string());
                    }
                } else {
                    warn!(%chat_id, item_id,
                        "no session_id in Q&A response — multi-turn will not work");
                }
                resp["answer"].as_str().unwrap_or("(no answer)").to_string()
            }
            Err(e) => {
                warn!(%chat_id, item_id, error = %e, "Q&A gateway call failed");
                let msg = format!("Q&A failed: {}", escape_html(&e.to_string()));
                let _ = self
                    .api
                    .edit_message_text(chat_id, ack_mid, &msg, Some("HTML"), None)
                    .await;
                return Ok(());
            }
        };

        if let Some(session) = self.qa_sessions.get_mut(chat_id) {
            session.rounds += 1;
        }

        let kb = crate::assistant::formatting::qa_session_kb(item_id);
        let msg = render_markdown_reply_html(&answer, 3800);

        if let Err(e) = self
            .api
            .edit_message_text(chat_id, ack_mid, &msg, Some("HTML"), Some(kb.clone()))
            .await
        {
            debug!(error = %e, "edit failed, sending new message");
            self.api
                .send_message(chat_id, &msg, Some("HTML"), Some(kb), true)
                .await?;
        }
        Ok(())
    }

    // ── Scout act sessions ──────────────────────────────────────────

    pub fn open_act_session(&mut self, cid: &str, item_id: i64, project: &str) {
        self.act_sessions.insert(
            cid.to_string(),
            ActSession {
                item_id,
                project: project.to_string(),
            },
        );
    }

    pub fn take_act_session(&mut self, cid: &str) -> Option<ActSession> {
        self.act_sessions.remove(cid)
    }
}
