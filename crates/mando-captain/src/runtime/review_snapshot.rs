//! Snapshot helper for fields mutated by `reset_review_retry`.

use mando_types::task::{ItemStatus, Task};

/// Snapshot of fields mutated by `reset_review_retry` (and status).
/// Used to roll back in-memory state if `persist_status_transition` fails.
pub(crate) struct ReviewFieldsSnapshot {
    pub status: ItemStatus,
    pub captain_review_trigger: Option<mando_types::task::ReviewTrigger>,
    pub review_session_id: Option<String>,
    pub review_fail_count: i64,
    pub last_activity_at: Option<String>,
}

impl ReviewFieldsSnapshot {
    /// Capture the current review-related fields before mutation.
    pub fn capture(item: &Task) -> Self {
        Self {
            status: item.status,
            captain_review_trigger: item.captain_review_trigger,
            review_session_id: item.session_ids.review.clone(),
            review_fail_count: item.review_fail_count,
            last_activity_at: item.last_activity_at.clone(),
        }
    }

    /// Restore all captured fields, undoing `reset_review_retry` + any status change.
    pub fn restore(self, item: &mut Task) {
        item.status = self.status;
        item.captain_review_trigger = self.captain_review_trigger;
        item.session_ids.review = self.review_session_id;
        item.review_fail_count = self.review_fail_count;
        item.last_activity_at = self.last_activity_at;
    }
}
