use serde::{Deserialize, Serialize};
use ts_rs::TS;

// ── Telegram keyboard primitives (replaces Value in NotificationPayload) ──

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TelegramReplyMarkup {
    InlineKeyboard {
        rows: Vec<Vec<InlineKeyboardButton>>,
    },
    ReplyKeyboard {
        rows: Vec<Vec<String>>,
        one_time: bool,
        resize: bool,
        persistent: bool,
    },
    ForceReply {},
    RemoveKeyboard {},
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InlineKeyboardButton {
    pub text: String,
    pub callback_data: Option<String>,
    pub url: Option<String>,
}

// ── Captain tick result (mirror of captain::TickResult) ───────────────

/// Mirror of `captain::ActionKind`. Variants + serde renames kept aligned
/// with the captain-side enum so round-trips stay stable.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub enum ActionKind {
    #[serde(rename = "skip")]
    Skip,
    #[serde(rename = "nudge")]
    Nudge,
    #[serde(rename = "captain-review")]
    CaptainReview,
}

/// Mirror of `captain::Action`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct TickAction {
    pub worker: String,
    pub action: ActionKind,
    pub message: Option<String>,
    pub reason: Option<String>,
}

/// Mirror of `captain::TickMode`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum TickMode {
    Live,
    DryRun,
    Skipped,
}

/// Mirror of `captain::TickResult`. Carries every field captain emits so
/// `/api/captain/tick` round-trips the raw payload without drift.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct TickResult {
    pub mode: TickMode,
    pub tick_id: Option<String>,
    pub max_workers: i64,
    pub active_workers: i64,
    pub tasks: std::collections::HashMap<String, i64>,
    pub alerts: Vec<String>,
    pub dry_actions: Vec<TickAction>,
    pub error: Option<String>,
    pub rate_limited: bool,
}

// ── Claude transcript line envelope (ndjson) ───────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TranscriptLine {
    Init(TranscriptInit),
    User(TranscriptUserEntry),
    Assistant(TranscriptAssistantEntry),
    ToolUse(TranscriptToolUse),
    ToolResult(TranscriptToolResult),
    Result(TranscriptResult),
    System(TranscriptSystem),
}

// `TranscriptLine` inner variants below. `deny_unknown_fields` is intentionally
// omitted: serde's internally-tagged enum deserialization passes the `type`
// discriminator into the inner deserializer, so a strict inner struct would
// reject it as an unknown field. Strictness is enforced at the enum level.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptInit {
    pub uuid: String,
    pub session_id: String,
    pub cwd: Option<String>,
    pub model: Option<String>,
    pub timestamp: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptUserEntry {
    pub uuid: String,
    pub parent_uuid: Option<String>,
    pub session_id: String,
    pub text: String,
    pub timestamp: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptAssistantEntry {
    pub uuid: String,
    pub parent_uuid: Option<String>,
    pub session_id: String,
    pub text: String,
    pub timestamp: Option<String>,
    pub usage: Option<crate::TranscriptUsageInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptToolUse {
    pub uuid: String,
    pub parent_uuid: Option<String>,
    pub tool_use_id: String,
    pub name: String,
    pub input_summary: String,
    pub timestamp: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptToolResult {
    pub uuid: String,
    pub parent_uuid: Option<String>,
    pub tool_use_id: String,
    pub is_error: bool,
    pub text: String,
    pub timestamp: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptResult {
    pub session_id: String,
    pub duration_ms: Option<i64>,
    pub cost_usd: Option<f64>,
    pub is_error: bool,
    pub error: Option<String>,
    pub timestamp: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptSystem {
    pub uuid: String,
    pub session_id: String,
    pub text: String,
    pub timestamp: Option<String>,
}

// ── Client log context (replaces Value in ClientLogEntry.context) ──────

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClientLogContext {
    pub source: Option<String>,
    pub component: Option<String>,
    pub file: Option<String>,
    pub line: Option<i64>,
    pub stack: Option<String>,
    pub session_id: Option<String>,
    pub route: Option<String>,
    pub extra: Option<String>,
}
