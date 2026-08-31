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

#[cfg(test)]
mod tests {
    use super::*;
    use time::Duration;

    fn meta_at(prompt_id: i64, secs_ago: i64) -> PromptMeta {
        PromptMeta {
            prompt_message_id: prompt_id,
            created_at: time::OffsetDateTime::now_utc() - Duration::seconds(secs_ago),
        }
    }

    #[test]
    fn pick_kind_returns_none_when_no_candidates() {
        assert_eq!(pick_kind(&[], None), None);
        assert_eq!(pick_kind(&[], Some(42)), None);
    }

    #[test]
    fn pick_kind_routes_to_reply_target_when_reply_matches() {
        let candidates = vec![
            (SessionKind::PendingTodo, meta_at(100, 30)),
            (SessionKind::QaSession, meta_at(200, 5)),
            (SessionKind::PendingNudge, meta_at(300, 60)),
        ];
        // Reply targets the older PendingTodo prompt, not the newer QaSession.
        let chosen = pick_kind(&candidates, Some(100));
        assert_eq!(chosen, Some(SessionKind::PendingTodo));
    }

    #[test]
    fn pick_kind_falls_back_to_most_recent_when_reply_does_not_match() {
        let candidates = vec![
            (SessionKind::PendingTodo, meta_at(100, 30)),
            (SessionKind::QaSession, meta_at(200, 5)),
            (SessionKind::PendingNudge, meta_at(300, 60)),
        ];
        // Reply id 999 matches nothing — fall back to most-recent (QaSession at 5s ago).
        let chosen = pick_kind(&candidates, Some(999));
        assert_eq!(chosen, Some(SessionKind::QaSession));
    }

    #[test]
    fn pick_kind_returns_most_recent_when_no_reply_target() {
        let candidates = vec![
            (SessionKind::PendingTodo, meta_at(100, 30)),
            (SessionKind::PendingNudge, meta_at(300, 60)),
            (SessionKind::QaSession, meta_at(200, 5)),
        ];
        // No reply context — pick the most recently created session.
        let chosen = pick_kind(&candidates, None);
        assert_eq!(chosen, Some(SessionKind::QaSession));
    }

    #[test]
    fn pick_kind_handles_single_candidate() {
        let candidates = vec![(SessionKind::PendingNudge, meta_at(42, 1))];
        assert_eq!(
            pick_kind(&candidates, None),
            Some(SessionKind::PendingNudge)
        );
        assert_eq!(
            pick_kind(&candidates, Some(42)),
            Some(SessionKind::PendingNudge)
        );
        assert_eq!(
            pick_kind(&candidates, Some(99)),
            Some(SessionKind::PendingNudge),
            "reply mismatch on lone candidate still routes to that candidate"
        );
    }

    #[tokio::test]
    async fn context_append_registry_reuses_task_lock_and_isolates_other_tasks() {
        let mut registry = PendingSessionRegistry::default();
        let first = registry.context_append_lock(42);
        let same_task = registry.context_append_lock(42);
        let other_task = registry.context_append_lock(43);

        assert!(Arc::ptr_eq(&first, &same_task));
        let guard = first.lock().await;
        assert!(same_task.try_lock().is_err());
        assert!(other_task.try_lock().is_ok());
        drop(guard);
        assert!(same_task.try_lock().is_ok());
    }
}

#[cfg(test)]
mod pending_command_tests {
    use super::*;
    use crate::http::GatewayClient;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    fn test_bot() -> TelegramBot {
        let config = Arc::new(RwLock::new(settings::Config::default()));
        let gw = GatewayClient::new(0, None);
        let pending = Arc::new(std::sync::Mutex::new(HashMap::new()));
        TelegramBot::with_base_url(config, "test-token", None, gw, pending)
            .expect("construct test bot")
    }

    async fn pending_timeline_has(bot: &TelegramBot, chat_id: &str) -> bool {
        bot.pending_sessions
            .lock()
            .await
            .chats
            .get(chat_id)
            .is_some_and(|sessions| sessions.timeline.is_some())
    }
    async fn pending_todo_has(bot: &TelegramBot, chat_id: &str) -> bool {
        bot.pending_sessions
            .lock()
            .await
            .chats
            .get(chat_id)
            .is_some_and(|sessions| sessions.todo.is_some())
    }
    async fn pending_scout_add_has(bot: &TelegramBot, chat_id: &str) -> bool {
        bot.pending_sessions
            .lock()
            .await
            .chats
            .get(chat_id)
            .is_some_and(|sessions| sessions.scout_add.is_some())
    }
    async fn pending_scout_research_has(bot: &TelegramBot, chat_id: &str) -> bool {
        bot.pending_sessions
            .lock()
            .await
            .chats
            .get(chat_id)
            .is_some_and(|sessions| sessions.scout_research.is_some())
    }
    async fn pending_reopen_has(bot: &TelegramBot, chat_id: &str) -> bool {
        bot.pending_sessions
            .lock()
            .await
            .chats
            .get(chat_id)
            .is_some_and(|sessions| sessions.reopen.is_some())
    }

    #[tokio::test]
    async fn set_pending_inserts_chat_id_with_prompt_meta() {
        let bot = test_bot();
        bot.set_pending_timeline("chat-1", 100).await;
        bot.set_pending_scout_add("chat-2", 200).await;
        bot.set_pending_scout_research("chat-3", 300).await;

        assert!(pending_timeline_has(&bot, "chat-1").await);
        assert!(pending_scout_add_has(&bot, "chat-2").await);
        assert!(pending_scout_research_has(&bot, "chat-3").await);

        let registry = bot.pending_sessions.lock().await;
        assert_eq!(
            registry
                .chats
                .get("chat-1")
                .and_then(|sessions| sessions.timeline.as_ref())
                .map(|meta| meta.prompt_message_id),
            Some(100)
        );
    }

    #[tokio::test]
    async fn take_pending_returns_and_removes() {
        let bot = test_bot();
        bot.set_pending_timeline("chat-1", 100).await;
        let taken = bot.take_pending_timeline("chat-1").await;
        assert_eq!(taken.map(|m| m.prompt_message_id), Some(100));
        assert!(!pending_timeline_has(&bot, "chat-1").await);
    }

    #[tokio::test]
    async fn reset_same_command_pending_clears_only_matching_command() {
        let bot = test_bot();
        bot.set_pending_timeline("chat-1", 100).await;
        bot.set_pending_scout_add("chat-1", 101).await;
        bot.set_pending_scout_research("chat-1", 102).await;

        // Re-issuing /timeline only resets pending_timeline.
        bot.reset_same_command_pending("chat-1", "timeline").await;
        assert!(!pending_timeline_has(&bot, "chat-1").await);
        assert!(
            pending_scout_add_has(&bot, "chat-1").await,
            "unrelated pending_scout_add must survive a /timeline re-issue"
        );
        assert!(
            pending_scout_research_has(&bot, "chat-1").await,
            "unrelated pending_scout_research must survive a /timeline re-issue"
        );
    }

    #[tokio::test]
    async fn reset_same_command_treats_history_as_timeline_alias() {
        let bot = test_bot();
        bot.set_pending_timeline("chat-1", 100).await;
        bot.reset_same_command_pending("chat-1", "history").await;
        assert!(
            !pending_timeline_has(&bot, "chat-1").await,
            "/history must reset pending_timeline (alias-equivalent)"
        );
    }

    #[tokio::test]
    async fn reset_same_command_pending_does_not_touch_callback_pendings() {
        // The concurrency-correct invariant: a fresh /command must not wipe
        // pendings that were opened by a callback (reopen / rework / nudge /
        // act / input / qa). Pre-fix, dispatch_text wiped these, which
        // killed in-flight follow-ups when a slow command was running.
        let bot = test_bot();
        bot.set_pending_reopen("chat-1", "42", "Fix bug", 500).await;
        bot.reset_same_command_pending("chat-1", "todo").await;
        assert!(
            pending_reopen_has(&bot, "chat-1").await,
            "callback-opened pending_reopen must survive a /todo command",
        );
    }

    #[tokio::test]
    async fn reset_same_command_pending_for_unknown_command_is_noop() {
        let bot = test_bot();
        bot.set_pending_timeline("chat-1", 100).await;
        bot.set_pending_scout_add("chat-1", 200).await;
        // /tasks owns no pending; resetting against it is a no-op.
        bot.reset_same_command_pending("chat-1", "tasks").await;
        assert!(pending_timeline_has(&bot, "chat-1").await);
        assert!(pending_scout_add_has(&bot, "chat-1").await);
    }

    #[tokio::test]
    async fn reset_same_command_pending_isolates_chats() {
        let bot = test_bot();
        bot.set_pending_timeline("chat-1", 100).await;
        bot.set_pending_timeline("chat-2", 200).await;

        bot.reset_same_command_pending("chat-1", "timeline").await;
        assert!(!pending_timeline_has(&bot, "chat-1").await);
        assert!(
            pending_timeline_has(&bot, "chat-2").await,
            "reset must not touch other chats"
        );
    }

    #[tokio::test]
    async fn concurrent_take_pending_todo_returns_some_to_exactly_one_caller() {
        // Race-correctness invariant: under spawn-per-update, two plain-text
        // updates from the same chat can both pick PendingTodo from the
        // disambiguation snapshot. Exactly one caller's `take_pending_todo`
        // must return `Some`; the other must observe the entry has already
        // been consumed (`None`) so its handler call site can fall through
        // to implicit URL detection instead of misrouting the text.
        let bot = std::sync::Arc::new(test_bot());
        bot.set_pending_todo("chat-1", 100).await;

        let bot_a = bot.clone();
        let bot_b = bot.clone();
        let (a, b) = tokio::join!(
            tokio::spawn(async move { bot_a.take_pending_todo("chat-1").await }),
            tokio::spawn(async move { bot_b.take_pending_todo("chat-1").await }),
        );
        let a = a.unwrap();
        let b = b.unwrap();

        // Exactly one Some, one None.
        let some_count = [a.is_some(), b.is_some()].iter().filter(|x| **x).count();
        assert_eq!(
            some_count,
            1,
            "exactly one concurrent caller must win the take race; got a={}, b={}",
            a.is_some(),
            b.is_some()
        );
        // And the entry is gone afterwards.
        assert!(!pending_todo_has(&bot, "chat-1").await);
        let cleanup = bot.take_pending_todo("chat-1").await;
        assert!(cleanup.is_none(), "entry must be fully drained after race");
    }
}
