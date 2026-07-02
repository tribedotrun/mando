//! Pending-session helpers and reply-disambiguation lookup.
//!
//! Each of the eight chat_id-keyed maps on [`TelegramBot`] stores a
//! `PromptMeta` so a plain-text reply can be routed to the matching session
//! either via `reply_to_message.message_id` or by the most-recent-wins
//! fallback. `pick_session_for_text` is the single disambiguation entry point.

use std::collections::HashMap;
use std::hash::Hash;

use anyhow::Result;
use tokio::sync::Mutex;
use tracing::{debug, warn};

use crate::bot::{
    ActSession, InputSession, PendingAction, PromptMeta, QaSession, Session, SessionKind,
    TelegramBot,
};
use crate::gateway_paths as paths;
use crate::telegram_format::{escape_html, render_markdown_reply_html};

/// Generates a `set_pending_*` / `take_pending_*` pair for a
/// `Mutex<HashMap<String, PromptMeta>>` field. Each text-command that opens
/// a no-args follow-up gets one — `/todo`, `/timeline`, `/scout_add`,
/// `/scout_research`.
macro_rules! prompt_pending_methods {
    ($set:ident, $take:ident, $field:ident) => {
        pub async fn $set(&self, chat_id: &str, prompt_message_id: i64) {
            self.$field
                .lock()
                .await
                .insert(chat_id.to_string(), PromptMeta::new(prompt_message_id));
        }
        pub async fn $take(&self, chat_id: &str) -> Option<PromptMeta> {
            self.$field.lock().await.remove(chat_id)
        }
    };
}

impl TelegramBot {
    // ── Text-command follow-ups (send args inline, or reply to prompt) ─

    prompt_pending_methods!(set_pending_todo, take_pending_todo, pending_todo);
    prompt_pending_methods!(
        set_pending_timeline,
        take_pending_timeline,
        pending_timeline
    );
    prompt_pending_methods!(
        set_pending_scout_add,
        take_pending_scout_add,
        pending_scout_add
    );
    prompt_pending_methods!(
        set_pending_scout_research,
        take_pending_scout_research,
        pending_scout_research
    );

    /// Reset only the pending entry that this `/command` re-opens. Other
    /// chat-scoped pendings (especially callback-opened ones — reopen,
    /// rework, nudge, ask, input, qa, act) survive concurrent dispatch.
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
        self.pending_reopen.lock().await.insert(
            chat_id.to_string(),
            PendingAction {
                item_id: item_id.to_string(),
                title: title.to_string(),
                prompt: PromptMeta::new(prompt_message_id),
            },
        );
    }

    pub async fn take_pending_reopen(&self, chat_id: &str) -> Option<PendingAction> {
        self.pending_reopen.lock().await.remove(chat_id)
    }

    pub async fn set_pending_rework(
        &self,
        chat_id: &str,
        item_id: &str,
        title: &str,
        prompt_message_id: i64,
    ) {
        self.pending_rework.lock().await.insert(
            chat_id.to_string(),
            PendingAction {
                item_id: item_id.to_string(),
                title: title.to_string(),
                prompt: PromptMeta::new(prompt_message_id),
            },
        );
    }

    pub async fn take_pending_rework(&self, chat_id: &str) -> Option<PendingAction> {
        self.pending_rework.lock().await.remove(chat_id)
    }

    pub async fn set_pending_nudge(
        &self,
        chat_id: &str,
        item_id: &str,
        title: &str,
        prompt_message_id: i64,
    ) {
        self.pending_nudge.lock().await.insert(
            chat_id.to_string(),
            PendingAction {
                item_id: item_id.to_string(),
                title: title.to_string(),
                prompt: PromptMeta::new(prompt_message_id),
            },
        );
    }

    pub async fn take_pending_nudge(&self, chat_id: &str) -> Option<PendingAction> {
        self.pending_nudge.lock().await.remove(chat_id)
    }

    // ── Input sessions ───────────────────────────────────────────────

    pub async fn has_input_session(&self, cid: &str) -> bool {
        self.input_sessions.lock().await.contains_key(cid)
    }
    pub async fn input_session_title(&self, cid: &str) -> Option<String> {
        let map = self.input_sessions.lock().await;
        map.get(cid).map(|s| s.title.clone())
    }
    pub async fn open_input_session(&self, cid: &str, title: &str, prompt_message_id: i64) {
        self.input_sessions.lock().await.insert(
            cid.to_string(),
            InputSession {
                title: title.to_string(),
                prompt: PromptMeta::new(prompt_message_id),
            },
        );
    }
    pub async fn close_input_session(&self, cid: &str) {
        self.input_sessions.lock().await.remove(cid);
    }

    // ── Ask sessions ─────────────────────────────────────────────────

    pub async fn has_ask_session(&self, cid: &str) -> bool {
        self.ask_sessions.lock().await.contains_key(cid)
    }
    pub async fn ask_session_rounds(&self, cid: &str) -> u32 {
        let map = self.ask_sessions.lock().await;
        map.get(cid).map(|s| s.rounds).unwrap_or(0)
    }
    pub async fn open_ask_session(&self, cid: &str, task_id: i64, prompt_message_id: i64) {
        // Close conflicting scout QA session so task-ask wins plain-text routing.
        self.qa_sessions.lock().await.remove(cid);
        self.ask_sessions.lock().await.insert(
            cid.to_string(),
            Session::new(task_id, PromptMeta::new(prompt_message_id)),
        );
    }
    pub async fn ask_session_task_id(&self, cid: &str) -> Option<i64> {
        self.ask_sessions.lock().await.get(cid).map(|s| s.task_id)
    }
    pub async fn close_ask_session(&self, cid: &str) {
        self.ask_sessions.lock().await.remove(cid);
    }
    pub async fn increment_ask_rounds(&self, cid: &str) {
        if let Some(s) = self.ask_sessions.lock().await.get_mut(cid) {
            s.rounds += 1;
        }
    }

    // ── Scout QA sessions ───────────────────────────────────────────

    pub async fn open_qa_session(&self, cid: &str, item_id: i64, prompt_message_id: i64) {
        // Close conflicting task-ask session so scout QA wins plain-text routing.
        self.ask_sessions.lock().await.remove(cid);
        self.qa_sessions.lock().await.insert(
            cid.to_string(),
            QaSession {
                item_id,
                rounds: 0,
                cc_session_id: None,
                prompt: PromptMeta::new(prompt_message_id),
            },
        );
    }

    pub async fn close_qa_session(&self, cid: &str) {
        self.qa_sessions.lock().await.remove(cid);
    }

    /// Returns `Ok(true)` when the QA session was found and the question
    /// dispatched; `Ok(false)` when the session has vanished between the
    /// disambiguation snapshot and consume (e.g. `endqa` callback ran on
    /// another spawned task) so the caller can fall through to implicit URL
    /// detection instead of silently dropping the user's message.
    pub(crate) async fn handle_qa_text(&self, chat_id: &str, question: &str) -> Result<bool> {
        // Snapshot under lock, then release before any HTTP call.
        let (item_id, cc_session_id) = {
            let map = self.qa_sessions.lock().await;
            match map.get(chat_id) {
                Some(s) => (s.item_id, s.cc_session_id.clone()),
                None => return Ok(false),
            }
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
        let result = self
            .gw
            .post_typed::<_, api_types::AskResponse>(paths::SCOUT_ASK, &body)
            .await;

        let answer = match result {
            Ok(resp) => {
                if let Some(ref sid) = resp.session_id {
                    let mut map = self.qa_sessions.lock().await;
                    if let Some(session) = map.get_mut(chat_id) {
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
            let mut map = self.qa_sessions.lock().await;
            if let Some(session) = map.get_mut(chat_id) {
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
        self.act_sessions.lock().await.insert(
            cid.to_string(),
            ActSession {
                item_id,
                project: project.to_string(),
                prompt: PromptMeta::new(prompt_message_id),
            },
        );
    }

    pub async fn take_act_session(&self, cid: &str) -> Option<ActSession> {
        self.act_sessions.lock().await.remove(cid)
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
        let mut out: Vec<(SessionKind, PromptMeta)> = Vec::new();
        push_meta(
            &mut out,
            &self.pending_todo,
            chat_id,
            SessionKind::PendingTodo,
        )
        .await;
        push_meta(
            &mut out,
            &self.pending_timeline,
            chat_id,
            SessionKind::PendingTimeline,
        )
        .await;
        push_meta(
            &mut out,
            &self.pending_scout_add,
            chat_id,
            SessionKind::PendingScoutAdd,
        )
        .await;
        push_meta(
            &mut out,
            &self.pending_scout_research,
            chat_id,
            SessionKind::PendingScoutResearch,
        )
        .await;
        push_meta(
            &mut out,
            &self.pending_reopen,
            chat_id,
            SessionKind::PendingReopen,
        )
        .await;
        push_meta(
            &mut out,
            &self.pending_rework,
            chat_id,
            SessionKind::PendingRework,
        )
        .await;
        push_meta(
            &mut out,
            &self.pending_nudge,
            chat_id,
            SessionKind::PendingNudge,
        )
        .await;
        push_meta(
            &mut out,
            &self.ask_sessions,
            chat_id,
            SessionKind::AskSession,
        )
        .await;
        push_meta(
            &mut out,
            &self.input_sessions,
            chat_id,
            SessionKind::InputSession,
        )
        .await;
        push_meta(&mut out, &self.qa_sessions, chat_id, SessionKind::QaSession).await;
        push_meta(
            &mut out,
            &self.act_sessions,
            chat_id,
            SessionKind::ActSession,
        )
        .await;
        out
    }
}

/// Trait that exposes a `PromptMeta` from each session-map value. Implemented
/// for every session value type so the snapshot helper is generic.
trait HasPromptMeta {
    fn prompt_meta(&self) -> &PromptMeta;
}

impl HasPromptMeta for PromptMeta {
    fn prompt_meta(&self) -> &PromptMeta {
        self
    }
}
impl HasPromptMeta for PendingAction {
    fn prompt_meta(&self) -> &PromptMeta {
        &self.prompt
    }
}
impl HasPromptMeta for InputSession {
    fn prompt_meta(&self) -> &PromptMeta {
        &self.prompt
    }
}
impl HasPromptMeta for Session {
    fn prompt_meta(&self) -> &PromptMeta {
        &self.prompt
    }
}
impl HasPromptMeta for QaSession {
    fn prompt_meta(&self) -> &PromptMeta {
        &self.prompt
    }
}
impl HasPromptMeta for ActSession {
    fn prompt_meta(&self) -> &PromptMeta {
        &self.prompt
    }
}

async fn push_meta<K, V>(
    out: &mut Vec<(SessionKind, PromptMeta)>,
    map: &Mutex<HashMap<K, V>>,
    chat_id: &str,
    kind: SessionKind,
) where
    K: Eq + Hash + std::borrow::Borrow<str>,
    V: HasPromptMeta,
{
    if let Some(meta) = map
        .lock()
        .await
        .get(chat_id)
        .map(|v| v.prompt_meta().clone())
    {
        out.push((kind, meta));
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
            (SessionKind::AskSession, meta_at(300, 60)),
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
            (SessionKind::AskSession, meta_at(300, 60)),
        ];
        // Reply id 999 matches nothing — fall back to most-recent (QaSession at 5s ago).
        let chosen = pick_kind(&candidates, Some(999));
        assert_eq!(chosen, Some(SessionKind::QaSession));
    }

    #[test]
    fn pick_kind_returns_most_recent_when_no_reply_target() {
        let candidates = vec![
            (SessionKind::PendingTodo, meta_at(100, 30)),
            (SessionKind::AskSession, meta_at(300, 60)),
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
        TelegramBot::new(config, "test-token", gw)
    }

    async fn pending_timeline_has(bot: &TelegramBot, chat_id: &str) -> bool {
        bot.pending_timeline.lock().await.contains_key(chat_id)
    }
    async fn pending_todo_has(bot: &TelegramBot, chat_id: &str) -> bool {
        bot.pending_todo.lock().await.contains_key(chat_id)
    }
    async fn pending_scout_add_has(bot: &TelegramBot, chat_id: &str) -> bool {
        bot.pending_scout_add.lock().await.contains_key(chat_id)
    }
    async fn pending_scout_research_has(bot: &TelegramBot, chat_id: &str) -> bool {
        bot.pending_scout_research
            .lock()
            .await
            .contains_key(chat_id)
    }
    async fn pending_reopen_has(bot: &TelegramBot, chat_id: &str) -> bool {
        bot.pending_reopen.lock().await.contains_key(chat_id)
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

        let timeline_entry = bot.pending_timeline.lock().await;
        assert_eq!(
            timeline_entry.get("chat-1").map(|m| m.prompt_message_id),
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
        // act / ask / input / qa). Pre-fix, dispatch_text wiped these, which
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
