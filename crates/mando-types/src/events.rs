//! Bus event types for SSE broadcasting.

use serde::{Deserialize, Serialize};

use crate::notify::NotifyLevel;

/// SSE event types broadcast on the event bus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BusEvent {
    #[serde(rename = "tasks")]
    Tasks,
    #[serde(rename = "scout")]
    Scout,
    #[serde(rename = "cron")]
    Cron,
    #[serde(rename = "status")]
    Status,
    #[serde(rename = "sessions")]
    Sessions,
    #[serde(rename = "notification")]
    Notification,
}

/// Consumer-agnostic notification payload.
///
/// Emitted on `BusEvent::Notification` — any client (TG, Electron, Slack)
/// can subscribe and render in its own format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationPayload {
    /// Human-readable message (may contain HTML for TG compatibility).
    pub message: String,
    /// Priority level.
    pub level: NotifyLevel,
    /// Semantic kind — consumers use this to build keyboards, deep links, etc.
    pub kind: NotificationKind,
    /// Stable key for edit-in-place (same key → edit previous message).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_key: Option<String>,
    /// Optional inline keyboard markup (TG JSON format).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_markup: Option<serde_json::Value>,
}

/// Semantic notification kinds — consumers interpret these to add
/// context-specific UI (keyboards, deep links, click actions).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum NotificationKind {
    /// Item moved to awaiting-review.
    AwaitingReview {
        item_id: String,
        pr_number: Option<u32>,
    },
    /// Item needs human clarification.
    ClarifierNeeded { item_id: String },
    /// Rebase failed after retries.
    RebaseFailed { item_id: String, pr_number: u32 },
    /// Worker crashed and exhausted retries — escalated to human.
    WorkerEscalated { item_id: String },
    /// Captain started reviewing a completed item.
    CaptainReviewStarted { item_id: String },
    /// Captain review verdict rendered (approve/rework/escalate).
    CaptainReviewVerdict {
        item_id: String,
        verdict: String,
        feedback: Option<String>,
    },
    /// Item escalated to human — includes CTO report summary.
    Escalated {
        item_id: String,
        summary: Option<String>,
    },
    /// Item errored during captain review.
    Errored {
        item_id: String,
        error: Option<String>,
    },
    /// Item needs human clarification — includes questions.
    NeedsClarification {
        item_id: String,
        questions: Option<String>,
    },
    /// Cron alert with action buttons.
    CronAlert { action_id: String },
    /// Claude API rate limit warning or rejection.
    RateLimited {
        status: String,
        utilization: Option<f64>,
        resets_at: Option<u64>,
        rate_limit_type: Option<String>,
        overage_status: Option<String>,
        overage_resets_at: Option<u64>,
        overage_disabled_reason: Option<String>,
    },
    /// Generic notification (no special UI).
    Generic,
}
