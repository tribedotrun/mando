//! Worker classification and health-state updates — extracted from `tick.rs` phase 2.

use mando_config::workflow::CaptainWorkflow;
use mando_types::captain::{Action, WorkerContext};
use mando_types::Task;

use crate::io::{health_store, health_store::HealthState};

/// Result of classifying all worker contexts in a tick.
pub(super) struct ClassifyResult {
    /// Actions to execute (live mode).
    pub actions_to_execute: Vec<Action>,
    /// Actions collected in dry-run mode.
    pub dry_actions: Vec<Action>,
}

/// Classify each worker context and update health state with current values.
///
/// For each worker, this:
/// 1. Looks up its task and stream result.
/// 2. Runs deterministic classification.
/// 3. Updates health state (cpu_time_s, cwd).
pub(super) fn classify_and_update_health(
    worker_contexts: &[WorkerContext],
    items: &[Task],
    health_state: &mut HealthState,
    workflow: &CaptainWorkflow,
    dry_run: bool,
) -> ClassifyResult {
    let mut actions_to_execute = Vec::new();
    let mut dry_actions = Vec::new();

    for ctx in worker_contexts {
        // Look up the task for this worker.
        let item_ref = items
            .iter()
            .find(|it| it.worker.as_deref() == Some(&ctx.session_name));

        // Get stream result for this worker via session_id.
        let cc_sid = item_ref.and_then(|it| it.session_ids.worker.as_deref());
        let stream_path = cc_sid.map(mando_config::stream_path_for_session);
        let stream_result = stream_path.as_deref().and_then(mando_cc::get_stream_result);
        let stream_clean = stream_result.as_ref().map(mando_cc::is_clean_result);
        let has_broken_session = stream_path
            .as_deref()
            .is_some_and(mando_cc::stream_has_broken_session);

        let action = match crate::biz::deterministic::classify_worker(
            ctx,
            item_ref,
            stream_clean,
            has_broken_session,
            &workflow.nudges,
            workflow.agent.worker_timeout_s,
            workflow.agent.stale_threshold_s,
            workflow.agent.max_interventions,
        ) {
            Some(a) => a,
            None => {
                tracing::error!(
                    module = "captain",
                    worker = %ctx.session_name,
                    "classify_worker returned None — skipping worker"
                );
                continue;
            }
        };

        if dry_run {
            dry_actions.push(action);
        } else {
            actions_to_execute.push(action);
        }

        // Update health state with current context values.
        if let Some(cpu) = ctx.cpu_time_s {
            health_store::set_health_field(
                health_state,
                &ctx.session_name,
                "cpu_time_s",
                serde_json::json!(cpu),
            );
        }

        // Persist worker CWD from the item's worktree field.
        let item_wt = items
            .iter()
            .find(|it| it.worker.as_deref() == Some(&ctx.session_name))
            .and_then(|it| it.worktree.as_deref());
        if let Some(wt) = item_wt {
            health_store::set_health_field(
                health_state,
                &ctx.session_name,
                "cwd",
                serde_json::json!(wt),
            );
        }
    }

    ClassifyResult {
        actions_to_execute,
        dry_actions,
    }
}
