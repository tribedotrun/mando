//! Worker classification and health-state updates. Extracted from tick.rs phase 2.

use crate::{Action, Task, WorkerContext};
use anyhow::Result;
use settings::CaptainWorkflow;

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
#[tracing::instrument(skip(worker_contexts, items, health_state, workflow, pool))]
pub(super) async fn classify_and_update_health(
    worker_contexts: &[WorkerContext],
    items: &[Task],
    health_state: &mut HealthState,
    workflow: &CaptainWorkflow,
    pool: &sqlx::SqlitePool,
    dry_run: bool,
) -> Result<ClassifyResult> {
    let mut actions_to_execute = Vec::new();
    let mut dry_actions = Vec::new();
    // One matcher per tick — lowercases and owns the configured rule list so
    // per-worker `detect()` calls stay cheap.
    let symptoms = global_claude::StreamSymptomMatcher::new(workflow.stream_symptoms.clone());

    for ctx in worker_contexts {
        // Look up the task for this worker.
        let item_ref = items
            .iter()
            .find(|it| it.worker.as_deref() == Some(&ctx.session_name));

        // Get stream result for this worker via session_id.
        let cc_sid = item_ref.and_then(|it| it.session_ids.worker.as_deref());
        let stream_path = item_ref.zip(cc_sid).map(|(item, sid)| {
            (
                sid,
                super::agent_runtime::implementation_provider(item, workflow),
            )
        });
        let stream_path = match stream_path {
            Some((sid, fallback_provider)) => {
                let provider =
                    super::agent_runtime::persisted_session_provider(pool, sid, fallback_provider)
                        .await;
                Some(super::agent_runtime::stream_path(provider, sid))
            }
            None => None,
        };
        let stream_result = stream_path
            .as_deref()
            .and_then(global_claude::get_stream_result);
        let stream_clean = stream_result.as_ref().map(global_claude::is_clean_result);
        let has_broken_session = stream_path
            .as_deref()
            .is_some_and(global_claude::stream_has_broken_session);

        let action = crate::service::deterministic::classify_worker(
            ctx,
            item_ref,
            stream_clean,
            has_broken_session,
            stream_path.as_deref(),
            &workflow.nudges,
            &symptoms,
            workflow.agent.worker_timeout_s,
            workflow.agent.stale_threshold_s,
            workflow.agent.max_interventions,
            workflow.agent.no_pr_min_active_s,
        )?;

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
        if let Some(wt) = item_ref.and_then(|it| it.worktree.as_deref()) {
            health_store::set_health_field(
                health_state,
                &ctx.session_name,
                "cwd",
                serde_json::json!(wt),
            );
        }
    }

    Ok(ClassifyResult {
        actions_to_execute,
        dry_actions,
    })
}
