use super::CaptainRuntime;

/// Convert a domain `Workbench` to the wire `api_types::WorkbenchItem` for
/// SSE bus broadcasts. Returns `Err` on schema drift so callers can
/// refuse to emit a corrupt event (fail-fast — the previous behavior of
/// emitting `item: None` papered over the drift and left the frontend
/// dispatching on a half-present payload).
pub fn to_wire_workbench_item(
    workbench: &crate::Workbench,
) -> anyhow::Result<api_types::WorkbenchItem> {
    let mut value = serde_json::to_value(workbench).map_err(|e| {
        anyhow::anyhow!(
            "failed to serialize Workbench {} for bus broadcast: {e}",
            workbench.id
        )
    })?;
    inject_worktree_exists(&mut value, &workbench.worktree, workbench.id)?;
    serde_json::from_value(value).map_err(|e| {
        anyhow::anyhow!(
            "failed to deserialize Workbench {} into api_types::WorkbenchItem (likely schema drift): {e}",
            workbench.id
        )
    })
}

/// Inject the derived `worktreeExists` field into a serialized workbench
/// JSON object. One source of truth for the filesystem probe used by every
/// `WorkbenchItem` wire path (HTTP routes, SSE snapshot, SSE bus broadcasts).
///
/// Semantics: returns `false` only when the daemon can affirmatively prove
/// the worktree is absent. Stat failures other than `NotFound` (permission,
/// stale NFS handle, unmounted volume) fail open to `true` with a warn log,
/// so a transient mount glitch never funnels the user into the
/// "Archive workbench" surface for a worktree they meant to keep.
fn inject_worktree_exists(
    value: &mut serde_json::Value,
    worktree: &str,
    workbench_id: i64,
) -> anyhow::Result<()> {
    let object = value.as_object_mut().ok_or_else(|| {
        anyhow::anyhow!("Workbench {workbench_id} did not serialize to a JSON object")
    })?;
    let exists = probe_worktree_exists(worktree, workbench_id);
    object.insert(
        "worktreeExists".to_string(),
        serde_json::Value::Bool(exists),
    );
    Ok(())
}

/// Filesystem probe for `inject_worktree_exists`. Sync `metadata` is
/// intentional: the call sites are bounded (one per workbench in a snapshot
/// or one per bus broadcast) and match the existing reconciler discipline.
fn probe_worktree_exists(worktree: &str, workbench_id: i64) -> bool {
    if worktree.is_empty() {
        return false;
    }
    match std::fs::metadata(worktree) {
        Ok(metadata) => metadata.is_dir(),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => false,
        Err(err) => {
            tracing::warn!(
                module = "captain-runtime-workbench_runtime",
                workbench_id,
                worktree,
                error = %err,
                "worktree stat failed for non-NotFound reason; reporting worktreeExists=true so users do not archive a worktree we could not check"
            );
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inject_marks_empty_path_as_missing() {
        let mut value = serde_json::json!({});
        inject_worktree_exists(&mut value, "", 7).unwrap();
        assert_eq!(value["worktreeExists"], serde_json::Value::Bool(false));
    }

    #[test]
    fn inject_marks_real_dir_as_present() {
        let tmp = tempfile::tempdir().unwrap();
        let mut value = serde_json::json!({});
        inject_worktree_exists(&mut value, tmp.path().to_str().unwrap(), 7).unwrap();
        assert_eq!(value["worktreeExists"], serde_json::Value::Bool(true));
    }

    #[test]
    fn inject_marks_missing_path_as_absent() {
        let mut value = serde_json::json!({});
        inject_worktree_exists(&mut value, "/this/path/should/never/exist/mando-test-42", 7)
            .unwrap();
        assert_eq!(value["worktreeExists"], serde_json::Value::Bool(false));
    }

    #[test]
    fn inject_errors_on_non_object_value() {
        let mut value = serde_json::Value::String("nope".into());
        let result = inject_worktree_exists(&mut value, "/tmp", 7);
        assert!(result.is_err(), "non-object input must fail loudly");
    }
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

/// Re-read a workbench and broadcast its current wire state without
/// touching `last_activity_at`. Used to push a fresh `worktreeExists`
/// value out-of-band when a route handler rejects a request (e.g. cwd
/// missing on terminal-create) so the renderer can transition to the
/// missing-worktree surface without waiting for the next snapshot.
///
/// Bumps `rev` first because the renderer's SSE cache patcher
/// (`patchListItem` in `sseCacheHelpers.ts`) rejects updates whose rev
/// is <= the cached one. Without the bump, the derived field flip is
/// silently dropped on the renderer side.
#[tracing::instrument(skip_all, fields(workbench_id))]
pub(super) async fn refresh_workbench_broadcast(runtime: &CaptainRuntime, workbench_id: i64) {
    if let Err(err) = crate::io::queries::workbenches::bump_rev(runtime.pool(), workbench_id).await
    {
        tracing::warn!(
            module = "captain-runtime-workbench_runtime",
            workbench_id,
            error = %err,
            "failed to bump workbench rev before refresh broadcast — continuing anyway"
        );
    }
    match crate::io::queries::workbenches::find_by_id(runtime.pool(), workbench_id).await {
        Ok(Some(workbench)) => match to_wire_workbench_item(&workbench) {
            Ok(item) => {
                runtime.bus().send(global_bus::BusPayload::Workbenches(Some(
                    api_types::WorkbenchEventData {
                        action: Some("updated".to_string()),
                        item: Some(item),
                    },
                )));
            }
            Err(err) => {
                tracing::error!(
                    module = "captain-runtime-workbench_runtime",
                    workbench_id,
                    error = %err,
                    "skipping workbench refresh broadcast — wire conversion failed"
                );
            }
        },
        Ok(None) => tracing::debug!(
            module = "captain-runtime-workbench_runtime",
            workbench_id,
            "workbench not found for refresh broadcast"
        ),
        Err(err) => tracing::warn!(
            module = "captain-runtime-workbench_runtime",
            workbench_id,
            error = %err,
            "failed to load workbench for refresh broadcast"
        ),
    }
}

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
