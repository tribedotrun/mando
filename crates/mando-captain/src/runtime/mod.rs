//! Runtime orchestration — composes biz + io.
//!
//! All functions are async.

pub mod action_contract;
pub mod ambient_rate_limit;
pub mod artifacts;
pub mod captain_merge;
mod captain_merge_poll;
mod captain_merge_spawn;
pub mod captain_review;
mod captain_review_check;
mod captain_review_error;
mod captain_review_helpers;
mod captain_review_verdict;
pub mod clarifier;
mod clarifier_validate;
pub mod credential_rate_limit;
pub mod dashboard;
pub mod dashboard_timeline;
pub mod dashboard_triage;
mod dispatch_clarify;
pub mod dispatch_phase;
mod dispatch_reclarify;
mod dispatch_redispatch;
pub mod mergeability;
mod mergeability_rebase;
mod mergeability_review;
pub mod notify;
mod nudge_health;
mod rebase_spawn;
pub mod reconciler;
mod reconciler_orphans;
pub mod review_phase;
mod review_phase_artifacts;
mod review_snapshot;
mod session_reconcile;
pub mod spawn_phase;
pub mod spawn_phase_review;
pub mod spawner;
pub mod spawner_lifecycle;
pub(crate) mod spawner_pr;
pub mod task_ask;
pub mod task_notes;
pub mod tick;
mod tick_action_loop;
mod tick_branch_sync;
mod tick_clarify_apply;
mod tick_clarify_poll;
mod tick_clarify_timeout;
mod tick_classify;
mod tick_guard;
pub mod tick_persist;
mod tick_post;
mod tick_review;
mod tick_rework;
pub mod tick_spawn;
pub mod timeline_backfill;
pub mod timeline_emit;

/// Revert a task to Queued, clearing all worker-related fields.
pub(crate) fn revert_to_queued(item: &mut mando_types::Task) {
    item.status = mando_types::task::ItemStatus::Queued;
    item.worker = None;
    item.session_ids.worker = None;
    item.session_ids.ask = None;
    item.worktree = None;
    // workbench_id is permanent — once assigned, never cleared.
    item.branch = None;
    item.worker_started_at = None;
}
