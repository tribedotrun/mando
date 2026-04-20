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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intervention_proceed() {
        let result = check_intervention(5, 1, 30);
        assert_eq!(result, InterventionResult::Proceed { new_count: 6 });
    }

    #[test]
    fn intervention_exhausted_at_boundary() {
        let result = check_intervention(29, 1, 30);
        assert_eq!(result, InterventionResult::Exhausted { new_count: 30 });
    }

    #[test]
    fn intervention_exhausted_over_boundary() {
        let result = check_intervention(30, 1, 30);
        assert_eq!(result, InterventionResult::Exhausted { new_count: 31 });
    }

    #[test]
    fn intervention_zero_budget() {
        let result = check_intervention(0, 1, 0);
        assert_eq!(result, InterventionResult::Exhausted { new_count: 1 });
    }

    #[test]
    fn intervention_multi_cost() {
        let result = check_intervention(3, 5, 10);
        assert_eq!(result, InterventionResult::Proceed { new_count: 8 });
    }

    #[test]
    fn intervention_multi_cost_exhausted() {
        let result = check_intervention(3, 5, 7);
        assert_eq!(result, InterventionResult::Exhausted { new_count: 8 });
    }

    #[test]
    fn ship_status_pr_item() {
        assert_eq!(ship_status(false), ItemStatus::AwaitingReview);
    }

    #[test]
    fn ship_status_no_pr_item() {
        assert_eq!(ship_status(true), ItemStatus::CompletedNoPr);
    }
}
