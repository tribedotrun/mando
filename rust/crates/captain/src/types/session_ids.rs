//! Session ID container — stores worker, review, clarifier, and ask CC session IDs as JSON.

use serde::{Deserialize, Serialize};

/// Session IDs for CC sessions a task can have.
/// Stored as a JSON TEXT column in SQLite. Any unknown fields in historical
/// rows (e.g. `triage` from the retired auto-merge-triage session) are
/// silently ignored by serde's default behavior.
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ask: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub advisor: Option<String>,
}

/// Which session id a write targets. Used by callers that learn the real
/// session id only after CC has spawned (a retried one-shot mints its own).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionSlot {
    Worker,
    Review,
    Clarifier,
    Merge,
    Ask,
    Advisor,
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
            SessionSlot::Ask => &self.ask,
            SessionSlot::Advisor => &self.advisor,
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
            SessionSlot::Ask => &mut self.ask,
            SessionSlot::Advisor => &mut self.advisor,
        };
        *target = Some(session_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_and_get_cover_every_slot() {
        for slot in [
            SessionSlot::Worker,
            SessionSlot::Review,
            SessionSlot::Clarifier,
            SessionSlot::Merge,
            SessionSlot::Ask,
            SessionSlot::Advisor,
        ] {
            let mut ids = SessionIds::default();
            assert_eq!(ids.get(slot), None);
            ids.set(slot, "sid-1".into());
            assert_eq!(ids.get(slot), Some("sid-1"));
            // Only the targeted slot moves.
            let json = ids.to_json();
            assert_eq!(json.matches("sid-1").count(), 1, "{json}");
        }
    }

    #[test]
    fn set_replaces_a_previous_value() {
        let mut ids = SessionIds::default();
        ids.set(SessionSlot::Review, "old".into());
        ids.set(SessionSlot::Review, "new".into());
        assert_eq!(ids.get(SessionSlot::Review), Some("new"));
    }
}
