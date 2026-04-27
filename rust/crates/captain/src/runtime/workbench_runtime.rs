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

/// Outcome of validating a renderer-supplied `workbench_id` for a
/// terminal-create. Lets the route map validation failures (NotFound /
/// WrongProject) to 400 and infrastructure failures (DB read errors)
/// to 500 without leaking raw error strings to the client.
#[derive(Debug)]
pub enum BindTerminalError {
    NotFound { workbench_id: i64 },
    WrongProject { workbench_id: i64 },
    Db(anyhow::Error),
}

impl std::fmt::Display for BindTerminalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BindTerminalError::NotFound { workbench_id } => {
                write!(f, "workbench {workbench_id} not found")
            }
            BindTerminalError::WrongProject { workbench_id } => {
                write!(f, "workbench {workbench_id} belongs to a different project")
            }
            BindTerminalError::Db(err) => write!(f, "{err}"),
        }
    }
}

impl std::error::Error for BindTerminalError {}

/// Validate the workbench the renderer claims to own a terminal-create
/// request, touch its activity, and broadcast the update. The renderer
/// is the source of truth: it knows from `useWorkbenchPage` which
/// workbench is foregrounded, and the server stamps that id onto the
/// session instead of re-deriving from cwd. cwd-based lookup leaks for
/// clarifier-resumed terminals (whose stored cwd is the project root,
/// not the worktree of any single workbench).
#[tracing::instrument(skip_all)]
pub(super) async fn bind_terminal_workbench(
    runtime: &CaptainRuntime,
    workbench_id: i64,
    project_name: &str,
) -> Result<(), BindTerminalError> {
    let workbench = crate::io::queries::workbenches::find_by_id(runtime.pool(), workbench_id)
        .await
        .map_err(BindTerminalError::Db)?
        .ok_or(BindTerminalError::NotFound { workbench_id })?;
    if workbench.project != project_name {
        return Err(BindTerminalError::WrongProject { workbench_id });
    }

    crate::io::queries::workbenches::touch_activity(runtime.pool(), workbench_id)
        .await
        .unwrap_or(false);
    match crate::io::queries::workbenches::find_by_id(runtime.pool(), workbench_id).await {
        Ok(Some(updated)) => match to_wire_workbench_item(&updated) {
            Ok(item) => {
                runtime.bus().send(global_bus::BusPayload::Workbenches(Some(
                    api_types::WorkbenchEventData {
                        action: Some("updated".to_string()),
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
                    workbench_id,
                    error = %err,
                    "skipping workbench bus broadcast — api-types schema drift"
                );
            }
        },
        Ok(None) => tracing::warn!(
            module = "captain-runtime-workbench_runtime",
            workbench_id,
            "workbench not found after activity touch"
        ),
        Err(err) => {
            tracing::warn!(module = "captain-runtime-workbench_runtime", workbench_id, error = %err, "failed to fetch workbench for bus broadcast")
        }
    }

    Ok(())
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
