//! Spawn logic — intervention budget and action processing.
//!
//! Pure business logic: determines state transitions without performing I/O.

use crate::ItemStatus;

/// Result of checking intervention budget.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InterventionResult {
    /// Budget allows this intervention.
    Proceed { new_count: u32 },
    /// Budget exhausted — needs captain review.
    Exhausted { new_count: u32 },
}

/// Check intervention budget. Each nudge costs +1.
pub(crate) fn check_intervention(
    current_count: u32,
    cost: u32,
    max_interventions: u32,
) -> InterventionResult {
    let new_count = current_count + cost;
    if new_count >= max_interventions {
        InterventionResult::Exhausted { new_count }
    } else {
        InterventionResult::Proceed { new_count }
    }
}

/// Determine new status for an awaiting-review / ship transition.
pub(crate) fn ship_status(is_no_pr: bool) -> ItemStatus {
    if is_no_pr {
        ItemStatus::CompletedNoPr
    } else {
        ItemStatus::AwaitingReview
    }
}
