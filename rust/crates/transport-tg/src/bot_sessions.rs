//! Pending-session helpers and reply-disambiguation lookup.
//!
//! One typed registry owns every chat-scoped pending flow plus task-scoped
//! context-append serialization. A single outer lock makes registry changes
//! atomic; per-task locks preserve concurrency across different tasks.
//! `pick_session_for_text` is the disambiguation entry point.

use std::collections::HashMap;
use std::sync::{Arc, Weak};

use anyhow::Result;
use tracing::{debug, warn};

use crate::bot::{
    ActSession, InputSession, PendingAction, PromptMeta, QaSession, SessionKind, TelegramBot,
};
use crate::telegram_format::{escape_html, render_markdown_reply_html};

#[derive(Default)]
pub(crate) struct PendingSessionRegistry {
    chats: HashMap<String, ChatPendingSessions>,
    context_append_locks: HashMap<i64, Weak<tokio::sync::Mutex<()>>>,
}

#[derive(Default)]
struct ChatPendingSessions {
    todo: Option<PromptMeta>,
    timeline: Option<PromptMeta>,
    scout_add: Option<PromptMeta>,
    scout_research: Option<PromptMeta>,
    reopen: Option<PendingAction>,
    rework: Option<PendingAction>,
    nudge: Option<PendingAction>,
    input: Option<InputSession>,
    qa: Option<QaSession>,
    act: Option<ActSession>,
}

impl PendingSessionRegistry {
    fn chat_mut(&mut self, chat_id: &str) -> &mut ChatPendingSessions {
        self.chats.entry(chat_id.to_string()).or_default()
    }

    fn cleanup_chat(&mut self, chat_id: &str) {
        if self
            .chats
            .get(chat_id)
            .is_some_and(ChatPendingSessions::is_empty)
        {
            self.chats.remove(chat_id);
        }
    }

    fn context_append_lock(&mut self, task_id: i64) -> Arc<tokio::sync::Mutex<()>> {
        self.context_append_locks
            .retain(|_, lock| lock.strong_count() > 0);
        if let Some(lock) = self
            .context_append_locks
            .get(&task_id)
            .and_then(Weak::upgrade)
        {
            return lock;
        }

        let lock = Arc::new(tokio::sync::Mutex::new(()));
        self.context_append_locks
            .insert(task_id, Arc::downgrade(&lock));
        lock
    }
}

impl ChatPendingSessions {
    fn is_empty(&self) -> bool {
        self.todo.is_none()
            && self.timeline.is_none()
            && self.scout_add.is_none()
            && self.scout_research.is_none()
            && self.reopen.is_none()
            && self.rework.is_none()
            && self.nudge.is_none()
            && self.input.is_none()
            && self.qa.is_none()
            && self.act.is_none()
    }

    fn prompt_candidates(&self) -> Vec<(SessionKind, PromptMeta)> {
        let mut candidates = Vec::new();
        let mut push = |kind, meta: Option<&PromptMeta>| {
            if let Some(meta) = meta {
                candidates.push((kind, meta.clone()));
            }
        };
        push(SessionKind::PendingTodo, self.todo.as_ref());
        push(SessionKind::PendingTimeline, self.timeline.as_ref());
        push(SessionKind::PendingScoutAdd, self.scout_add.as_ref());
        push(
            SessionKind::PendingScoutResearch,
            self.scout_research.as_ref(),
        );
        push(
            SessionKind::PendingReopen,
            self.reopen.as_ref().map(|session| &session.prompt),
        );
        push(
            SessionKind::PendingRework,
            self.rework.as_ref().map(|session| &session.prompt),
        );
        push(
            SessionKind::PendingNudge,
            self.nudge.as_ref().map(|session| &session.prompt),
        );
        push(
            SessionKind::InputSession,
            self.input.as_ref().map(|session| &session.prompt),
        );
        push(
            SessionKind::QaSession,
            self.qa.as_ref().map(|session| &session.prompt),
        );
        push(
            SessionKind::ActSession,
            self.act.as_ref().map(|session| &session.prompt),
        );
        candidates
    }
}

/// Generates a `set_pending_*` / `take_pending_*` pair for one typed slot.
macro_rules! prompt_pending_methods {
    ($set:ident, $take:ident, $field:ident) => {
        pub async fn $set(&self, chat_id: &str, prompt_message_id: i64) {
            self.pending_sessions.lock().await.chat_mut(chat_id).$field =
                Some(PromptMeta::new(prompt_message_id));
        }
        pub async fn $take(&self, chat_id: &str) -> Option<PromptMeta> {
            let mut registry = self.pending_sessions.lock().await;
            let result = registry
                .chats
                .get_mut(chat_id)
                .and_then(|sessions| sessions.$field.take());
            registry.cleanup_chat(chat_id);
            result
        }
    };
}

impl TelegramBot {
    // ── Text-command follow-ups (send args inline, or reply to prompt) ─

    prompt_pending_methods!(set_pending_todo, take_pending_todo, todo);
    prompt_pending_methods!(set_pending_timeline, take_pending_timeline, timeline);
    prompt_pending_methods!(set_pending_scout_add, take_pending_scout_add, scout_add);
    prompt_pending_methods!(
        set_pending_scout_research,
        take_pending_scout_research,
        scout_research
    );

    /// Reset only the pending entry that this `/command` re-opens. Other
    /// chat-scoped pendings (especially callback-opened ones — reopen,
    /// rework, nudge, input, qa, act) survive concurrent dispatch.
    /// `/timeline` ↔ `/history` count as the same command.
    pub(crate) async fn reset_same_command_pending(&self, chat_id: &str, command: &str) {
        match command {
            "todo" => {
                self.take_pending_todo(chat_id).await;
            }
            "timeline" | "history" => {
                self.take_pending_timeline(chat_id).await;
            }
            "scout_add" => {
                self.take_pending_scout_add(chat_id).await;
            }
            "scout_research" => {
                self.take_pending_scout_research(chat_id).await;
            }
            _ => {}
        }
    }

    // ── Captain-action follow-ups (reopen / rework / nudge) ──────────

    pub async fn set_pending_reopen(
        &self,
        chat_id: &str,
        item_id: &str,
        title: &str,
        prompt_message_id: i64,
    ) {
        self.pending_sessions.lock().await.chat_mut(chat_id).reopen = Some(PendingAction {
            item_id: item_id.to_string(),
            title: title.to_string(),
            prompt: PromptMeta::new(prompt_message_id),
        });
    }

    pub async fn take_pending_reopen(&self, chat_id: &str) -> Option<PendingAction> {
        let mut registry = self.pending_sessions.lock().await;
        let result = registry
            .chats
            .get_mut(chat_id)
            .and_then(|sessions| sessions.reopen.take());
        registry.cleanup_chat(chat_id);
        result
    }

    pub async fn set_pending_rework(
        &self,
        chat_id: &str,
        item_id: &str,
        title: &str,
        prompt_message_id: i64,
    ) {
        self.pending_sessions.lock().await.chat_mut(chat_id).rework = Some(PendingAction {
            item_id: item_id.to_string(),
            title: title.to_string(),
            prompt: PromptMeta::new(prompt_message_id),
        });
    }

    pub async fn take_pending_rework(&self, chat_id: &str) -> Option<PendingAction> {
        let mut registry = self.pending_sessions.lock().await;
        let result = registry
            .chats
            .get_mut(chat_id)
            .and_then(|sessions| sessions.rework.take());
        registry.cleanup_chat(chat_id);
        result
    }

    pub async fn set_pending_nudge(
        &self,
        chat_id: &str,
        item_id: &str,
        title: &str,
        prompt_message_id: i64,
    ) {
        self.pending_sessions.lock().await.chat_mut(chat_id).nudge = Some(PendingAction {
            item_id: item_id.to_string(),
            title: title.to_string(),
            prompt: PromptMeta::new(prompt_message_id),
        });
    }

    pub async fn take_pending_nudge(&self, chat_id: &str) -> Option<PendingAction> {
        let mut registry = self.pending_sessions.lock().await;
        let result = registry
            .chats
            .get_mut(chat_id)
            .and_then(|sessions| sessions.nudge.take());
        registry.cleanup_chat(chat_id);
        result
    }

    // ── Input sessions ───────────────────────────────────────────────

    pub async fn has_input_session(&self, cid: &str) -> bool {
        self.pending_sessions
            .lock()
            .await
            .chats
            .get(cid)
            .is_some_and(|sessions| sessions.input.is_some())
    }
    pub async fn input_session_title(&self, cid: &str) -> Option<String> {
        self.pending_sessions
            .lock()
            .await
            .chats
            .get(cid)
            .and_then(|sessions| sessions.input.as_ref())
            .map(|session| session.title.clone())
    }
    pub async fn input_session(&self, cid: &str) -> Option<InputSession> {
        self.pending_sessions
            .lock()
            .await
            .chats
            .get(cid)
            .and_then(|sessions| sessions.input.clone())
    }
    pub async fn open_input_session(
        &self,
        cid: &str,
        task_id: i64,
        title: &str,
        prompt_message_id: i64,
    ) {
        self.pending_sessions.lock().await.chat_mut(cid).input = Some(InputSession {
            task_id,
            title: title.to_string(),
            prompt: PromptMeta::new(prompt_message_id),
        });
    }
    pub async fn close_input_session(&self, cid: &str) {
        let mut registry = self.pending_sessions.lock().await;
        if let Some(sessions) = registry.chats.get_mut(cid) {
            sessions.input = None;
        }
        registry.cleanup_chat(cid);
    }

    pub(crate) async fn lock_context_append(
        &self,
        task_id: i64,
    ) -> tokio::sync::OwnedMutexGuard<()> {
        let lock = self
            .pending_sessions
            .lock()
            .await
            .context_append_lock(task_id);
        lock.lock_owned().await
    }

    // ── Scout QA sessions ───────────────────────────────────────────

    pub async fn open_qa_session(&self, cid: &str, item_id: i64, prompt_message_id: i64) {
        let mut registry = self.pending_sessions.lock().await;
        let sessions = registry.chat_mut(cid);
        sessions.qa = Some(QaSession {
            item_id,
            rounds: 0,
            cc_session_id: None,
            prompt: PromptMeta::new(prompt_message_id),
        });
    }

    pub async fn close_qa_session(&self, cid: &str) {
        let mut registry = self.pending_sessions.lock().await;
        if let Some(sessions) = registry.chats.get_mut(cid) {
            sessions.qa = None;
        }
        registry.cleanup_chat(cid);
    }

    /// Returns `Ok(true)` when the QA session was found and the question
    /// dispatched; `Ok(false)` when the session has vanished between the
    /// disambiguation snapshot and consume (e.g. `endqa` callback ran on
    /// another spawned task) so the caller can fall through to implicit URL
    /// detection instead of silently dropping the user's message.
    pub(crate) async fn handle_qa_text(&self, chat_id: &str, question: &str) -> Result<bool> {
        // Snapshot under lock, then release before any HTTP call.
        let (item_id, cc_session_id) = {
            let registry = self.pending_sessions.lock().await;
            match registry
                .chats
                .get(chat_id)
                .and_then(|sessions| sessions.qa.as_ref())
            {
                Some(session) => (session.item_id, session.cc_session_id.clone()),
                None => return Ok(false),
            }
        };

        let ack = self
            .api
            .send_message(chat_id, "\u{1f4ac} Thinking\u{2026}", None, None, true)
            .await?;
        let ack_mid = ack.get("message_id").and_then(|v| v.as_i64()).unwrap_or(0);

        let result = self
            .gw
            .post_scout_ask(&api_types::ScoutAskRequest {
                id: item_id,
                question: question.to_string(),
                session_id: cc_session_id,
            })
            .await;

        let answer = match result {
            Ok(resp) => {
                if let Some(ref sid) = resp.session_id {
                    let mut registry = self.pending_sessions.lock().await;
                    if let Some(session) = registry
                        .chats
                        .get_mut(chat_id)
                        .and_then(|sessions| sessions.qa.as_mut())
                    {
                        session.cc_session_id = Some(sid.clone());
                    }
                } else {
                    warn!(%chat_id, item_id,
                        "no session_id in Q&A response — multi-turn will not work");
                }
                resp.answer.clone()
            }
            Err(e) => {
                warn!(%chat_id, item_id, error = %e, "Q&A gateway call failed");
                let msg = format!("Q&A failed: {}", escape_html(&e.to_string()));
                global_infra::best_effort!(
                    self.api
                        .edit_message_text(chat_id, ack_mid, &msg, Some("HTML"), None)
                        .await,
                    "bot_sessions: edit_message on Q&A error"
                );
                return Ok(true);
            }
        };

        {
            let mut registry = self.pending_sessions.lock().await;
            if let Some(session) = registry
                .chats
                .get_mut(chat_id)
                .and_then(|sessions| sessions.qa.as_mut())
            {
                session.rounds += 1;
            }
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
        Ok(true)
    }

    // ── Scout act sessions ──────────────────────────────────────────

    pub async fn open_act_session(
        &self,
        cid: &str,
        item_id: i64,
        project: &str,
        prompt_message_id: i64,
    ) {
        self.pending_sessions.lock().await.chat_mut(cid).act = Some(ActSession {
            item_id,
            project: project.to_string(),
            prompt: PromptMeta::new(prompt_message_id),
        });
    }

    pub async fn take_act_session(&self, cid: &str) -> Option<ActSession> {
        let mut registry = self.pending_sessions.lock().await;
        let result = registry
            .chats
            .get_mut(cid)
            .and_then(|sessions| sessions.act.take());
        registry.cleanup_chat(cid);
        result
    }

    // ── Reply-disambiguation lookup ─────────────────────────────────

    /// Pick which pending session should consume a plain-text reply.
    ///
    /// Priority:
    ///   1. If `reply_to_message_id` is `Some` and any session's
    ///      `prompt_message_id` matches, route to that session.
    ///   2. Otherwise, route to the most-recently-created session
    ///      (most-recent-wins).
    ///   3. If no session exists for this chat, return `None` so the caller
    ///      can fall through to implicit URL detection.
    pub(crate) async fn pick_session_for_text(
        &self,
        chat_id: &str,
        reply_to_message_id: Option<i64>,
    ) -> Option<SessionKind> {
        let snapshot = self.snapshot_prompt_meta(chat_id).await;
        pick_kind(&snapshot, reply_to_message_id)
    }

    async fn snapshot_prompt_meta(&self, chat_id: &str) -> Vec<(SessionKind, PromptMeta)> {
        self.pending_sessions
            .lock()
            .await
            .chats
            .get(chat_id)
            .map(ChatPendingSessions::prompt_candidates)
            .unwrap_or_default()
    }
}

fn pick_kind(
    candidates: &[(SessionKind, PromptMeta)],
    reply_to_message_id: Option<i64>,
) -> Option<SessionKind> {
    if candidates.is_empty() {
        return None;
    }
    if let Some(reply_id) = reply_to_message_id {
        if let Some((kind, _)) = candidates
            .iter()
            .find(|(_, meta)| meta.prompt_message_id == reply_id)
        {
            return Some(*kind);
        }
    }
    // Most-recent-wins fallback.
    candidates
        .iter()
        .max_by_key(|(_, meta)| meta.created_at)
        .map(|(kind, _)| *kind)
}
