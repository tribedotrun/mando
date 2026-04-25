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

/// Why the drain loop exited. Wire-named, closed set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum DrainStop {
    /// A tick reported zero state changes from the previous pass.
    Idle,
    /// Hit the iteration ceiling (request-level or server-clamped).
    MaxTicks,
    /// Hit the wall-clock ceiling before the requested condition met.
    WallClock,
    /// Target task reached one of the `until_status` values.
    UntilStatus,
    /// Daemon cancellation fired mid-drain.
    Cancelled,
}

/// Response for `/api/captain/tick`. Wraps the final iteration's `TickResult`
/// with drain metadata. A caller that supplied no drain-triggering fields
/// still receives this shape with `iterations = 1` and
/// `stopped_reason = max-ticks` — single-tick is just drain-with-cap-1.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct TickDrainResult {
    pub iterations: u32,
    pub stopped_reason: DrainStop,
    /// Wall-clock duration of the drain, in milliseconds.
    pub elapsed_ms: u64,
    /// Final tick's `TickResult`. Always present — even a 0-iteration drain
    /// (no-op request) surfaces the last tick it ran, and the guarded empty
    /// result when nothing ran.
    pub last: TickResult,
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
