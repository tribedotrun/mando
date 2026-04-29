//! Session-state and picker types for the Telegram bot.
//!
//! Extracted from `bot.rs` for file length and to keep `bot.rs` focused on
//! the polling/dispatch surface.

use std::collections::HashSet;

use time::OffsetDateTime;

/// Records when (and against which outgoing message) a pending session was
/// registered. Used by the reply-disambiguation lookup to route plain-text
/// replies to the correct session under concurrent command dispatch.
#[derive(Debug, Clone)]
pub struct PromptMeta {
    /// `message_id` of the bot's prompt message — what the user replies to.
    pub prompt_message_id: i64,
    /// When the entry was registered (UTC). Most-recent-wins fallback when
    /// no `reply_to_message` is present on the inbound text.
    pub created_at: OffsetDateTime,
}

impl PromptMeta {
    pub fn new(prompt_message_id: i64) -> Self {
        Self {
            prompt_message_id,
            created_at: OffsetDateTime::now_utc(),
        }
    }
}

/// Captain-action follow-up awaiting human feedback (reopen/rework/nudge).
#[derive(Debug, Clone)]
pub struct PendingAction {
    pub item_id: String,
    pub title: String,
    pub prompt: PromptMeta,
}

/// Active task-clarify input session; the title is shown back to the human.
#[derive(Debug, Clone)]
pub struct InputSession {
    pub title: String,
    pub prompt: PromptMeta,
}

/// Lightweight session tracker for `/ask` (per-task Q&A).
#[derive(Debug, Clone)]
pub struct Session {
    pub task_id: i64,
    pub rounds: u32,
    pub prompt: PromptMeta,
}

impl Session {
    pub fn new(task_id: i64, prompt: PromptMeta) -> Self {
        Self {
            task_id,
            rounds: 0,
            prompt,
        }
    }
}

/// Active scout-item Q&A session.
#[derive(Debug, Clone)]
pub struct QaSession {
    pub item_id: i64,
    pub rounds: u32,
    /// CC session ID from first Q&A response — used to resume on follow-ups.
    pub cc_session_id: Option<String>,
    pub prompt: PromptMeta,
}

/// Pending Act session — waiting for optional user prompt (scout items).
#[derive(Debug, Clone)]
pub struct ActSession {
    pub item_id: i64,
    pub project: String,
    pub prompt: PromptMeta,
}

/// Picker state stored while an inline keyboard is active.
#[derive(Debug)]
pub struct PickerState {
    pub chat_id: String,
    pub items: Vec<PickerItem>,
    /// Indices of selected items (for multi-select pickers).
    pub selected: HashSet<usize>,
}

/// One item in a picker.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct PickerItem {
    pub id: String,
    pub title: String,
    /// Item status string (e.g. "needs-clarification").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Whether this task has a PR attached.
    pub has_pr: bool,
}

/// One parsed todo item with optional project assignment.
#[derive(Debug, Clone)]
pub struct TodoItem {
    pub title: String,
    /// Project slug resolved via prefix match or single-project auto-select.
    pub project: Option<String>,
    /// Telegram photo file_id (highest-res) — only set on first item.
    pub photo_file_id: Option<String>,
}

/// Pending /todo confirmation state.
#[derive(Debug)]
pub struct TodoConfirmState {
    pub chat_id: String,
    pub items: Vec<TodoItem>,
    /// Ordered project slugs for the picker (indices used in callback_data).
    pub picker_slugs: Vec<String>,
}

/// One of the chat_id-keyed pending session maps.
///
/// Used by the reply-disambiguation lookup to identify which map's entry
/// matches the user's reply target (`reply_to_message.message_id`) or
/// most-recent-wins fallback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SessionKind {
    PendingTodo,
    PendingTimeline,
    PendingScoutAdd,
    PendingScoutResearch,
    PendingReopen,
    PendingRework,
    PendingNudge,
    AskSession,
    InputSession,
    QaSession,
    ActSession,
}
