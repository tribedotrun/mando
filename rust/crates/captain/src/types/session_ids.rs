//! Session ID container for task-owned agent sessions.

use serde::{Deserialize, Serialize};

/// Session IDs for CC sessions a task can have.
/// Stored as a JSON TEXT column in SQLite. Any unknown fields in historical
/// rows (including retired slots) are silently ignored by serde's default
/// behavior.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionIds {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worker: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub review: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clarifier: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub merge: Option<String>,
}

/// Which session id a write targets. Used by callers that learn the real
/// session id only after CC has spawned (a retried one-shot mints its own).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionSlot {
    Worker,
    Review,
    Clarifier,
    Merge,
}

impl SessionIds {
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".into())
    }

    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }

    pub fn get(&self, slot: SessionSlot) -> Option<&str> {
        let value = match slot {
            SessionSlot::Worker => &self.worker,
            SessionSlot::Review => &self.review,
            SessionSlot::Clarifier => &self.clarifier,
            SessionSlot::Merge => &self.merge,
        };
        value.as_deref()
    }

    /// Point one slot at `session_id`. The value must be an id CC has
    /// actually produced or adopted — never a locally minted UUID (see the
    /// `session_ids` invariant in CLAUDE.md).
    pub fn set(&mut self, slot: SessionSlot, session_id: String) {
        let target = match slot {
            SessionSlot::Worker => &mut self.worker,
            SessionSlot::Review => &mut self.review,
            SessionSlot::Clarifier => &mut self.clarifier,
            SessionSlot::Merge => &mut self.merge,
        };
        *target = Some(session_id);
    }
}
