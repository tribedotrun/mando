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
    #[serde(rename = "status")]
    Status,
    #[serde(rename = "sessions")]
    Sessions,
    #[serde(rename = "notification")]
    Notification,
    #[serde(rename = "config")]
    Config,
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
    /// Item escalated to human — includes CTO report summary.
    Escalated {
        item_id: String,
        summary: Option<String>,
    },
    /// Item needs human clarification — includes questions.
    NeedsClarification {
        item_id: String,
        questions: Option<String>,
    },
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
    /// Scout item finished processing.
    ScoutProcessed {
        scout_id: i64,
        title: String,
        relevance: i64,
        quality: i64,
        source_name: Option<String>,
        telegraph_url: Option<String>,
    },
    /// Scout item processing failed.
    ScoutProcessFailed {
        scout_id: i64,
        url: String,
        error: String,
    },
    /// Generic notification (no special UI).
    Generic,
}
