//! Runtime orchestration — composes biz + io.
//!
//! All functions are async.

pub mod action_contract;
mod agent_liveness;
mod agent_nudge;
pub(crate) mod agent_runtime;
mod agent_session_result;
mod agent_worker_runtime;
pub mod ambient_rate_limit;
pub mod captain_merge;
mod captain_merge_poll;
mod captain_merge_spawn;
pub mod captain_review;
mod captain_review_check;
mod captain_review_error;
mod captain_review_evidence;
mod captain_review_helpers;
mod captain_review_payload;
mod captain_review_verdict;
pub mod clarifier;
mod clarifier_cc_failure;
pub mod clarifier_reclarify;
mod clarifier_session;
mod claude_clarifier_session;
mod claude_detached_session;
pub(crate) mod claude_worker_control;
pub(crate) mod codex_app_server;
mod codex_app_server_watch;
pub(crate) mod codex_output_schema;
mod codex_session;
pub(crate) mod codex_stream;
pub(crate) mod codex_structured;
pub(crate) mod codex_worker_control;
pub(crate) mod codex_worker_prompt;
pub(crate) mod codex_worker_spawn;
mod credential_codex_warmup;
pub mod credential_rate_limit;
pub mod credential_usage_poll;
pub mod daemon;
pub mod dashboard;
pub mod dashboard_timeline;
pub mod dashboard_triage;
mod dispatch_clarify;
pub mod dispatch_phase;
mod dispatch_redispatch;
pub(crate) mod lifecycle_effects;
mod merge_session;
pub mod mergeability;
mod mergeability_auto_merge;
mod mergeability_rebase;
mod mergeability_review;
pub mod notify;
mod nudge_health;
pub(crate) mod opencode_worker_spawn;
pub(crate) mod opencode_worker_stream;
mod rebase_session;
mod rebase_spawn;
pub mod reconciler;
mod reconciler_orphans;
pub mod review_phase;
mod review_phase_artifacts;
mod review_session;
mod review_snapshot;
mod session_reconcile;
mod session_retarget;
pub mod spawn_phase;
pub mod spawn_phase_review;
pub mod spawner;
pub mod spawner_lifecycle;
pub(crate) mod spawner_pr;
pub(crate) mod spawner_prompt;
mod startup_session_reconcile;
pub mod task_creation;
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
mod tick_rate_limit;
mod tick_review;
mod tick_rework;
pub mod tick_spawn;
pub mod timeline_emit;
mod worker_checkout;
pub mod worker_exit;

pub use daemon::CaptainRuntime;

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
    item.branch = None;
    item.worker_started_at = None;
}
