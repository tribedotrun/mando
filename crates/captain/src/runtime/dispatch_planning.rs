//! Planning mode dispatch and polling.
//!
//! Planning tasks are intercepted at dispatch time (Queued + planning=true)
//! and routed to the planning pipeline instead of a regular worker.
//! The pipeline runs as a background tokio task and writes its result to a
//! stream file. Completion is polled on subsequent ticks.

use std::panic::AssertUnwindSafe;

use crate::{ItemStatus, Task, TimelineEventType};
use futures::FutureExt;
use global_bus::EventBus;
use global_types::BusEvent;
use settings::config::settings::Config;
use settings::config::workflow::CaptainWorkflow;

/// Dispatch planning-mode items. Returns the number of items dispatched.
///
/// Called from `dispatch_phase` before the regular worker dispatch loop.
/// Planning items are pulled out of the Queued pool so they don't count
/// toward regular worker slots.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn dispatch_planning_items(
    items: &mut [Task],
    config: &Config,
    workflow: &CaptainWorkflow,
    pool: &sqlx::SqlitePool,
    bus: Option<&EventBus>,
    dry_run: bool,
    dry_actions: &mut Vec<String>,
) -> usize {
    let mut dispatched = 0;

    for item in items
        .iter_mut()
        .filter(|it| it.planning && it.status == ItemStatus::Queued)
    {
        if dry_run {
            dry_actions.push(format!(
                "would start planning pipeline for '{}'",
                crate::runtime::dashboard::truncate_utf8(&item.title, 60)
            ));
            dispatched += 1;
            continue;
        }

        // Resolve cwd before persisting to avoid inconsistent DB state on failure.
        let cwd = match super::planning::resolve_planning_cwd(item, config) {
            Ok(c) => c,
            Err(e) => {
                tracing::error!(module = "planning", id = item.id, error = %e, "cannot resolve cwd");
                continue;
            }
        };

        let session_id = global_infra::uuid::Uuid::v4().to_string();
        item.status = ItemStatus::InProgress;
        item.worker = Some(format!("planning-{}", &session_id[..8]));
        item.session_ids.worker = Some(session_id.clone());
        item.worker_started_at = Some(global_types::now_rfc3339());
        item.last_activity_at = Some(global_types::now_rfc3339());

        // Persist immediately so UI shows the running planning session.
        if let Err(e) = crate::io::queries::tasks::persist_spawn(pool, item).await {
            tracing::error!(
                module = "planning",
                id = item.id,
                error = %e,
                "failed to persist planning dispatch"
            );
            super::revert_to_queued(item);
            continue;
        }
        let _ = crate::io::headless_cc::log_cc_session(
            pool,
            &crate::io::headless_cc::SessionLogEntry {
                session_id: &session_id,
                cwd: &cwd,
                model: &workflow.models.captain,
                caller: "planning",
                cost_usd: None,
                duration_ms: None,
                resumed: false,
                task_id: Some(item.id),
                status: global_types::SessionStatus::Running,
                worker_name: item.worker.as_deref().unwrap_or(""),
                credential_id: None,
            },
        )
        .await;

        let _ = super::timeline_emit::emit_for_task(
            item,
            TimelineEventType::WorkerSpawned,
            &format!("Planning pipeline started ({})", item.worker.as_deref().unwrap_or("")),
            serde_json::json!({"worker": item.worker, "session_id": &session_id, "mode": "planning"}),
            pool,
        )
        .await;

        // Spawn the planning pipeline as a background task.
        let task_clone = item.clone();
        let workflow = workflow.clone();
        let config = config.clone();
        let pool = pool.clone();
        let sid = session_id.clone();
        let sid_for_panic = session_id.clone();

        tokio::spawn(async move {
            let result = AssertUnwindSafe(async {
                match super::planning::run_planning_pipeline(&task_clone, &workflow, &config, &pool)
                    .await
                {
                    Ok(plan_result) => {
                        // Emit PlanCompleted BEFORE the stream file so the
                        // timeline event is in the DB before the poller detects
                        // completion and emits PlanReady.
                        let _ = super::timeline_emit::emit(
                            &pool,
                            task_clone.id,
                            TimelineEventType::PlanCompleted,
                            "captain",
                            "Planning complete",
                            serde_json::json!({
                                "diagram": plan_result.diagram,
                                "plan": plan_result.plan,
                            }),
                        )
                        .await;

                        // Write completion marker after the timeline event
                        // is persisted, so the poller sees it in order.
                        if let Err(e) = write_planning_result(&sid, &plan_result) {
                            tracing::error!(
                                module = "planning",
                                task_id = task_clone.id,
                                error = %e,
                                "failed to write result stream file"
                            );
                            write_planning_error(&sid, &format!("failed to write result: {e}"));
                        }

                        tracing::info!(
                            module = "planning",
                            task_id = task_clone.id,
                            %sid,
                            "planning pipeline completed successfully"
                        );
                    }
                    Err(e) => {
                        tracing::error!(
                            module = "planning",
                            task_id = task_clone.id,
                            %sid,
                            error = %e,
                            "planning pipeline failed"
                        );
                        write_planning_error(&sid, &format!("{e}"));
                    }
                }
            })
            .catch_unwind()
            .await;

            if let Err(panic) = result {
                tracing::error!(
                    module = "planning",
                    session_id = %sid_for_panic,
                    "planning pipeline panicked: {:?}",
                    panic
                );
                write_planning_error(
                    &sid_for_panic,
                    &format!("planning pipeline panicked: {:?}", panic),
                );
            }
        });

        dispatched += 1;

        if let Some(bus) = bus {
            bus.send(BusEvent::Tasks, None);
            bus.send(
                BusEvent::Sessions,
                Some(serde_json::json!({"affected_task_ids": [item.id]})),
            );
        }
    }

    dispatched
}

/// Poll InProgress planning items for completion.
pub(crate) async fn poll_planning_items(items: &mut [Task], pool: &sqlx::SqlitePool) {
    for item in items
        .iter_mut()
        .filter(|it| it.planning && it.status == ItemStatus::InProgress)
    {
        let Some(ref session_id) = item.session_ids.worker else {
            continue;
        };

        let stream_path = global_infra::paths::stream_path_for_session(session_id);
        let result = match global_claude::get_stream_result(&stream_path) {
            Some(r) => r,
            None => continue, // Not done yet.
        };

        // Check if it was an error.
        if result.get("is_error").and_then(|v| v.as_bool()) == Some(true) {
            let error_msg = result
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("planning pipeline failed");
            tracing::error!(
                module = "planning",
                task_id = item.id,
                %error_msg,
                "planning pipeline errored"
            );
            item.status = ItemStatus::Errored;
            item.escalation_report = Some(format!("Planning pipeline failed: {error_msg}"));
            item.last_activity_at = Some(global_types::now_rfc3339());

            // Emit timeline event so the Feed view shows what went wrong.
            let _ = super::timeline_emit::emit_for_task(
                item,
                TimelineEventType::Errored,
                &format!("Planning failed: {error_msg}"),
                serde_json::json!({"mode": "planning", "error": error_msg}),
                pool,
            )
            .await;

            // Mark session as failed so it doesn't stay "running" forever.
            if let Err(e) = crate::io::headless_cc::log_session_completion(
                pool,
                session_id,
                "",
                "planning",
                item.worker.as_deref().unwrap_or(""),
                Some(item.id),
                global_types::SessionStatus::Failed,
            )
            .await
            {
                tracing::warn!(module = "planning", error = %e, "failed to log error session");
            }
            continue;
        }

        // Success -- transition to PlanReady (user reviews, then triggers impl).
        tracing::info!(
            module = "planning",
            task_id = item.id,
            "planning pipeline completed, transitioning to PlanReady"
        );
        item.status = ItemStatus::PlanReady;
        item.last_activity_at = Some(global_types::now_rfc3339());

        let _ = super::timeline_emit::emit_for_task(
            item,
            TimelineEventType::PlanReady,
            "Plan ready for review",
            serde_json::json!({"mode": "planning"}),
            pool,
        )
        .await;

        // Mark session as stopped.
        if let Err(e) = crate::io::headless_cc::log_session_completion(
            pool,
            session_id,
            "",
            "planning",
            item.worker.as_deref().unwrap_or(""),
            Some(item.id),
            global_types::SessionStatus::Stopped,
        )
        .await
        {
            tracing::warn!(module = "planning", error = %e, "failed to log session completion");
        }
    }
}

/// Write a successful planning result as a synthetic stream result.
fn write_planning_result(
    session_id: &str,
    result: &super::planning::PlanningResult,
) -> std::io::Result<()> {
    let stream_path = global_infra::paths::stream_path_for_session(session_id);
    if let Some(parent) = stream_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let envelope = serde_json::json!({
        "type": "result",
        "subtype": "success",
        "result": format!("Planning complete.\n\n{}\n\n{}", result.diagram, result.plan),
    });
    let line = serde_json::to_string(&envelope).unwrap_or_default();
    std::fs::write(&stream_path, format!("{line}\n"))
}

/// Write an error result as a synthetic stream result.
fn write_planning_error(session_id: &str, error: &str) {
    let stream_path = global_infra::paths::stream_path_for_session(session_id);
    if let Some(parent) = stream_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    global_claude::write_error_result(&stream_path, error);
}

/// Revert InProgress planning tasks to Queued on startup. Planning pipelines
/// run as in-process tokio tasks that are killed on daemon exit, so any
/// InProgress+planning task after restart is an orphan.
pub(crate) async fn reconcile_orphaned_planning(pool: &sqlx::SqlitePool) {
    match crate::io::queries::tasks_persist::revert_orphaned_planning(pool).await {
        Ok(n) if n > 0 => {
            tracing::info!(
                module = "reconciler",
                count = n,
                "reverted orphaned planning tasks to queued"
            );
        }
        Err(e) => {
            tracing::error!(
                module = "reconciler",
                error = %e,
                "failed to reconcile orphaned planning tasks"
            );
        }
        _ => {}
    }
}
