//! Runtime orchestration — composes biz + io.
//!
//! All functions are async.

pub mod action_contract;
pub mod ambient_rate_limit;
pub mod captain_merge;
mod captain_merge_poll;
mod captain_merge_spawn;
pub mod captain_review;
mod captain_review_check;
mod captain_review_error;
mod captain_review_helpers;
mod captain_review_payload;
mod captain_review_verdict;
pub mod clarifier;
mod clarifier_cc_failure;
pub mod clarifier_reclarify;
pub mod credential_rate_limit;
pub mod credential_usage_poll;
pub mod daemon;
pub mod dashboard;
pub mod dashboard_timeline;
pub mod dashboard_triage;
mod dispatch_clarify;
pub mod dispatch_phase;
pub(crate) mod dispatch_planning;
mod dispatch_redispatch;
pub(crate) mod lifecycle_effects;
pub mod mergeability;
mod mergeability_auto_merge;
mod mergeability_rebase;
mod mergeability_review;
pub mod notify;
mod nudge_health;
pub(crate) mod planning;
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
pub(crate) mod spawner_prompt;
mod startup_session_reconcile;
pub mod task_ask;
pub mod task_notes;
pub mod tick;
mod tick_action_loop;
mod tick_branch_sync;
pub mod tick_clarify_apply;
mod tick_clarify_poll;
mod tick_clarify_timeout;
mod tick_classify;
mod tick_guard;
pub mod tick_persist;
mod tick_post;
mod tick_review;
mod tick_rework;
pub mod tick_spawn;
pub mod timeline_emit;
pub mod worker_exit;

pub use daemon::CaptainRuntime;

pub(crate) fn clear_task_interaction_sessions(item: &mut crate::Task) {
    item.session_ids.ask = None;
    item.session_ids.advisor = None;
}

/// Revert a task to Queued, clearing all worker-related fields.
///
/// Worktree and workbench_id are permanent once assigned — captain
/// invariant #4 in CLAUDE.md. The next spawn reuses the same worktree
/// via the spawner's Rework arm.
pub(crate) fn revert_to_queued(item: &mut crate::Task) {
    global_infra::best_effort!(
        crate::service::lifecycle::apply_transition(item, crate::ItemStatus::Queued),
        "mod: crate::service::lifecycle::apply_transition(item, crate::Ite"
    );
    item.worker = None;
    item.session_ids.worker = None;
    clear_task_interaction_sessions(item);
    item.branch = None;
    item.worker_started_at = None;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ItemStatus, Task};

    #[test]
    fn revert_to_queued_clears_ask_and_advisor_sessions() {
        let mut item = Task::new("cleanup");
        item.set_status_for_tests(ItemStatus::InProgress);
        item.worker = Some("worker".into());
        item.session_ids.worker = Some("worker-sid".into());
        item.session_ids.ask = Some("ask-sid".into());
        item.session_ids.advisor = Some("advisor-sid".into());

        revert_to_queued(&mut item);

        assert_eq!(item.status(), ItemStatus::Queued);
        assert!(item.session_ids.worker.is_none());
        assert!(item.session_ids.ask.is_none());
        assert!(item.session_ids.advisor.is_none());
    }

    #[test]
    fn revert_to_queued_preserves_worktree_and_workbench() {
        // Captain invariant #4 in CLAUDE.md — same task, same worktree
        // (and same workbench). revert_to_queued rolls back worker fields
        // after a failed persist_spawn, but the task's persistent worktree
        // binding must survive so the next spawn hits the spawner's Rework
        // arm.
        let mut item = Task::new("cleanup");
        item.set_status_for_tests(ItemStatus::InProgress);
        item.worker = Some("worker".into());
        item.session_ids.worker = Some("worker-sid".into());
        item.worktree = Some("/tmp/mando-todo-42".into());
        item.branch = Some("mando/todo-42".into());
        item.workbench_id = 7;

        revert_to_queued(&mut item);

        assert_eq!(
            item.worktree.as_deref(),
            Some("/tmp/mando-todo-42"),
            "worktree must survive revert_to_queued",
        );
        assert_eq!(item.workbench_id, 7, "workbench_id is permanent");
        assert!(item.branch.is_none(), "branch is cleared");
        assert!(item.worker.is_none(), "worker is cleared");
    }
}
