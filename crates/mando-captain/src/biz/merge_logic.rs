//! Merge conflict detection logic — pure helpers.

use mando_types::task::{ItemStatus, Task};

/// Identify pending-review items that need a rebase check.
///
/// Criteria: status is pending-review, has a PR, no active rebase worker,
/// and rebase hasn't already failed.
pub(crate) fn items_needing_rebase_check(items: &[Task]) -> Vec<usize> {
    items
        .iter()
        .enumerate()
        .filter(|(_, item)| {
            item.status == ItemStatus::AwaitingReview
                && item.pr.is_some()
                && item.rebase_worker.is_none()
        })
        .map(|(i, _)| i)
        .collect()
}

/// Identify handed-off items with PRs that need merge/close watching.
///
/// Human owns the work, but we still detect when their PR merges or closes
/// so the task state stays accurate.
pub(crate) fn items_needing_merge_watch(items: &[Task]) -> Vec<usize> {
    items
        .iter()
        .enumerate()
        .filter(|(_, item)| item.status == ItemStatus::HandedOff && item.pr.is_some())
        .map(|(i, _)| i)
        .collect()
}

/// Check if an item's rebase worker has failed.
pub(crate) fn is_rebase_failed(item: &Task) -> bool {
    item.rebase_worker.as_deref() == Some("failed")
}

/// Compute the next rebase retry count.
pub(crate) fn next_rebase_retry(item: &Task) -> u32 {
    item.rebase_retries as u32 + 1
}

/// Exponential backoff delay for rebase retries: base_s * 2^(retries-1).
/// Returns 0 for the first attempt (no delay).
pub(crate) fn rebase_delay_s(retries: u32, base_s: u64) -> u64 {
    if retries == 0 {
        return 0;
    }
    base_s.saturating_mul(1u64 << (retries - 1).min(10))
}

/// Check whether a rebase succeeded by comparing the current branch HEAD SHA
/// against the SHA recorded before the rebase worker was spawned.
/// If the SHA changed, the worker successfully pushed — even if main moved again
/// and the PR is now conflicting with a *new* conflict.
pub(crate) fn did_rebase_succeed(old_sha: Option<&str>, current_sha: &str) -> bool {
    match old_sha {
        Some(old) => old != current_sha,
        None => false, // no baseline → can't tell, treat as failure
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_pr_item(status: ItemStatus) -> Task {
        let mut item = Task::new("Test");
        item.status = status;
        item.pr = Some("42".into());
        item
    }

    #[test]
    fn pending_review_needs_rebase() {
        let item = make_pr_item(ItemStatus::AwaitingReview);
        let indices = items_needing_rebase_check(&[item]);
        assert_eq!(indices, vec![0]);
    }

    #[test]
    fn active_rebase_worker_excluded() {
        let mut item = make_pr_item(ItemStatus::AwaitingReview);
        item.rebase_worker = Some("mando-rebase-0".into());
        let indices = items_needing_rebase_check(&[item]);
        assert!(indices.is_empty());
    }

    #[test]
    fn handed_off_merge_watch() {
        let item = make_pr_item(ItemStatus::HandedOff);
        let indices = items_needing_merge_watch(&[item]);
        assert_eq!(indices, vec![0]);
    }

    #[test]
    fn handed_off_no_pr_not_watched() {
        let mut item = Task::new("Test");
        item.status = ItemStatus::HandedOff;
        let indices = items_needing_merge_watch(&[item]);
        assert!(indices.is_empty());
    }

    #[test]
    fn awaiting_review_not_merge_watched() {
        let item = make_pr_item(ItemStatus::AwaitingReview);
        let indices = items_needing_merge_watch(&[item]);
        assert!(indices.is_empty());
    }

    #[test]
    fn in_progress_not_checked() {
        let item = make_pr_item(ItemStatus::InProgress);
        let indices = items_needing_rebase_check(&[item]);
        assert!(indices.is_empty());
    }

    #[test]
    fn rebase_failed_detected() {
        let mut item = Task::new("T");
        item.rebase_worker = Some("failed".into());
        assert!(is_rebase_failed(&item));
    }

    #[test]
    fn next_retry_increments() {
        let mut item = Task::new("T");
        item.rebase_retries = 2;
        assert_eq!(next_rebase_retry(&item), 3);
    }

    #[test]
    fn next_retry_from_none() {
        let item = Task::new("T");
        assert_eq!(next_rebase_retry(&item), 1);
    }

    #[test]
    fn rebase_delay_first_attempt() {
        assert_eq!(rebase_delay_s(0, 30), 0);
    }

    #[test]
    fn rebase_delay_exponential() {
        assert_eq!(rebase_delay_s(1, 30), 30);
        assert_eq!(rebase_delay_s(2, 30), 60);
        assert_eq!(rebase_delay_s(3, 30), 120);
        assert_eq!(rebase_delay_s(4, 30), 240);
        assert_eq!(rebase_delay_s(5, 30), 480);
    }

    #[test]
    fn rebase_success_sha_changed() {
        assert!(did_rebase_succeed(Some("abc123"), "def456"));
    }

    #[test]
    fn rebase_failure_sha_unchanged() {
        assert!(!did_rebase_succeed(Some("abc123"), "abc123"));
    }

    #[test]
    fn rebase_no_baseline() {
        assert!(!did_rebase_succeed(None, "abc123"));
    }
}
