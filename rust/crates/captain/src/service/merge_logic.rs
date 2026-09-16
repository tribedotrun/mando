//! Merge conflict detection logic — pure helpers.

use crate::{ItemStatus, Task};

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
                && item.pr_number.is_some()
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
        .filter(|(_, item)| item.status == ItemStatus::HandedOff && item.pr_number.is_some())
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

/// Exponential backoff delay for rebase retries: base * 2^(retries-1).
/// Returns Duration::ZERO for the first attempt (no delay).
pub(crate) fn rebase_delay(retries: u32, base: std::time::Duration) -> std::time::Duration {
    if retries == 0 {
        return std::time::Duration::ZERO;
    }
    let multiplier = 1u32 << (retries - 1).min(10);
    base.saturating_mul(multiplier)
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
