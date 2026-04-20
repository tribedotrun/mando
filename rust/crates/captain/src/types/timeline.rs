//! Timeline event types for task history.
//!
//! The wire type — [`TimelineEventPayload`], a tagged discriminated union —
//! is defined in `api-types::models_wire` and re-exported here so captain
//! producers and the HTTP layer share one type. The captain-side
//! [`TimelineEvent`] wraps it with the common envelope fields (`timestamp`,
//! `actor`, `summary`) used for DB persistence.

pub use api_types::TimelineEventPayload;
use serde::{Deserialize, Serialize};

/// A single event in a task's timeline.
///
/// `data` carries a [`TimelineEventPayload`] variant whose fields are
/// exactly the keys this event kind emits. The serialized `event_type`
/// discriminator lives inside the payload via the `#[serde(tag =
/// "event_type")]` annotation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineEvent {
    pub timestamp: String,
    pub actor: String,
    pub summary: String,
    pub data: TimelineEventPayload,
}

impl TimelineEvent {
    /// Convenience — return the event-type discriminator string that
    /// matches the `timeline_events.event_type` column.
    pub fn event_type_str(&self) -> &'static str {
        self.data.event_type_str()
    }
}
