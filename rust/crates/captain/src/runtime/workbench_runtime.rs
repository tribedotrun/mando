use tracing::warn;

use super::CaptainRuntime;

/// Convert a domain `Workbench` to the wire `api_types::WorkbenchItem` for
/// SSE bus broadcasts. Returns `Err` on schema drift so callers can
/// refuse to emit a corrupt event (fail-fast — the previous behavior of
/// emitting `item: None` papered over the drift and left the frontend
/// dispatching on a half-present payload).
pub(crate) fn to_wire_workbench_item(
    workbench: &crate::Workbench,
) -> anyhow::Result<api_types::WorkbenchItem> {
    let value = serde_json::to_value(workbench).map_err(|e| {
        anyhow::anyhow!(
            "failed to serialize Workbench {} for bus broadcast: {e}",
            workbench.id
        )
    })?;
    serde_json::from_value(value).map_err(|e| {
        anyhow::anyhow!(
            "failed to deserialize Workbench {} into api_types::WorkbenchItem (likely schema drift): {e}",
            workbench.id
        )
    })
}

#[tracing::instrument(skip_all)]
pub(super) async fn prepare_terminal_workbench(
    runtime: &CaptainRuntime,
    project_name: &str,
    cwd: &str,
    is_resume: bool,
) -> anyhow::Result<Option<i64>> {
    let project_id = match settings::projects::find_by_name(runtime.pool(), project_name).await {
        Ok(Some(row)) => row.id,
        Ok(None) => settings::projects::upsert(runtime.pool(), project_name, "", None).await?,
        Err(err) => return Err(err),
    };

    let existing = crate::io::queries::workbenches::find_by_worktree(runtime.pool(), cwd).await?;
    let (new_workbench_id, broadcast_id) = match (is_resume, existing.as_ref()) {
        (false, None) => {
            let title = crate::workbench_title_now();
            let workbench =
                crate::Workbench::new(project_id, project_name.to_string(), cwd.to_string(), title);
            let id = crate::io::queries::workbenches::insert(runtime.pool(), &workbench).await?;
            (Some(id), Some(id))
        }
        (_, Some(workbench)) => {
            let touched =
                crate::io::queries::workbenches::touch_activity(runtime.pool(), workbench.id)
                    .await
                    .unwrap_or(false);
            (None, touched.then_some(workbench.id))
        }
        _ => (None, None),
    };

    if let Some(id) = broadcast_id {
        let action = if new_workbench_id.is_some() {
            "created"
        } else {
            "updated"
        };
        match crate::io::queries::workbenches::find_by_id(runtime.pool(), id).await {
            Ok(Some(updated)) => match to_wire_workbench_item(&updated) {
                Ok(item) => {
                    runtime.bus().send(global_bus::BusPayload::Workbenches(Some(
                        api_types::WorkbenchEventData {
                            action: Some(action.to_string()),
                            item: Some(item),
                        },
                    )));
                }
                Err(err) => {
                    // DB row is already committed. The SSE broadcast is
                    // observability only — log-and-skip instead of
                    // propagating, otherwise the caller sees 500 and
                    // retries a mutation that already succeeded.
                    tracing::error!(
                        module = "captain-runtime-workbench_runtime",
                        workbench_id = id,
                        error = %err,
                        "skipping workbench bus broadcast — api-types schema drift"
                    );
                }
            },
            Ok(None) => tracing::warn!(
                module = "captain-runtime-workbench_runtime",
                workbench_id = id,
                "workbench not found after activity touch"
            ),
            Err(err) => {
                tracing::warn!(module = "captain-runtime-workbench_runtime", workbench_id = id, error = %err, "failed to fetch workbench for bus broadcast")
            }
        }
    }

    Ok(new_workbench_id)
}

#[tracing::instrument(skip_all)]
pub(super) async fn rollback_terminal_workbench(runtime: &CaptainRuntime, workbench_id: i64) {
    if let Err(err) = crate::io::queries::workbenches::archive(runtime.pool(), workbench_id).await {
        warn!(module = "captain", workbench_id, error = %err, "failed to archive workbench after terminal creation failure");
    }
}

#[tracing::instrument(skip_all)]
pub(super) async fn record_terminal_cc_session(
    runtime: &CaptainRuntime,
    cwd: &str,
    cc_session_id: &str,
) -> anyhow::Result<()> {
    if let Some(workbench) =
        crate::io::queries::workbenches::find_by_worktree(runtime.pool(), cwd).await?
    {
        let has_tasks =
            crate::io::queries::tasks::has_active_for_workbench(runtime.pool(), workbench.id)
                .await
                .unwrap_or(false);
        if !has_tasks {
            global_infra::best_effort!(
                crate::io::queries::workbenches::set_pending_title_session(
                    runtime.pool(),
                    workbench.id,
                    cc_session_id,
                )
                .await,
                "workbench_runtime: crate::io::queries::workbenches::set_pending_title_session( "
            );
        }
    }
    Ok(())
}

#[tracing::instrument(skip_all)]
pub(super) async fn notify_terminal_activity(
    runtime: &CaptainRuntime,
    cwd: &str,
) -> anyhow::Result<bool> {
    let Some(workbench) =
        crate::io::queries::workbenches::find_by_worktree(runtime.pool(), cwd).await?
    else {
        return Ok(false);
    };
    let touched =
        crate::io::queries::workbenches::touch_activity(runtime.pool(), workbench.id).await?;
    if touched {
        match crate::io::queries::workbenches::find_by_id(runtime.pool(), workbench.id).await {
            Ok(Some(updated)) => match to_wire_workbench_item(&updated) {
                Ok(item) => {
                    runtime.bus().send(global_bus::BusPayload::Workbenches(Some(
                        api_types::WorkbenchEventData {
                            action: Some("updated".into()),
                            item: Some(item),
                        },
                    )));
                }
                Err(err) => {
                    // touch_activity is already committed; skip the SSE
                    // broadcast on schema drift rather than failing the
                    // caller's mutation retroactively.
                    tracing::error!(
                        module = "captain-runtime-workbench_runtime",
                        workbench_id = workbench.id,
                        error = %err,
                        "skipping workbench bus broadcast — api-types schema drift"
                    );
                }
            },
            Ok(None) => tracing::warn!(
                module = "captain-runtime-workbench_runtime",
                workbench_id = workbench.id,
                "workbench not found after activity touch"
            ),
            Err(err) => {
                tracing::warn!(module = "captain-runtime-workbench_runtime", workbench_id = workbench.id, error = %err, "failed to fetch workbench for bus broadcast")
            }
        }
    }
    runtime.auto_title_notify().notify_one();
    Ok(touched)
}
