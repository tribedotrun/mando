//! Bus event types for SSE broadcasting.

use serde::{Deserialize, Serialize};

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
    #[serde(rename = "workbenches")]
    Workbenches,
    #[serde(rename = "config")]
    Config,
    #[serde(rename = "research")]
    Research,
    #[serde(rename = "credentials")]
    Credentials,
    #[serde(rename = "artifacts")]
    Artifacts,
}
