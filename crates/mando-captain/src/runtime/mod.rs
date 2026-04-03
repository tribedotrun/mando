//! Runtime orchestration — composes biz + io.
//!
//! All functions are async.

pub mod action_contract;
pub mod captain_merge;
pub mod captain_review;
mod captain_review_verdict;
pub mod clarifier;
mod clarifier_validate;
pub mod dashboard;
pub mod dashboard_timeline;
pub mod dashboard_triage;
pub mod dispatch_phase;
mod dispatch_reclarify;
mod dispatch_redispatch;
pub mod distiller;
pub mod guardian;
pub mod linear_integration;
pub mod mergeability;
mod mergeability_rebase;
mod mergeability_review;
pub mod notify;
pub mod rate_limit_cooldown;
pub mod reconciler;
pub mod review_phase;
mod session_reconcile;
pub mod spawn_phase;
pub mod spawn_phase_review;
pub mod spawner;
pub mod spawner_lifecycle;
pub mod task_ask;
pub mod task_notes;
pub mod tick;
mod tick_action_loop;
mod tick_clarify_timeout;
mod tick_classify;
mod tick_guard;
pub mod tick_journal;
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
    item.worktree = None;
    item.branch = None;
    item.worker_started_at = None;
}
