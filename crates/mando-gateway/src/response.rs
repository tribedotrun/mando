//! HTTP response helpers for route handlers.
//!
//! Stateless helpers re-exported from transport-http.
//! Stateful helpers (AppState-dependent) defined here.

use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

use crate::AppState;

pub(crate) use transport_http::response::{
    error_response, internal_error, internal_error_with, map_task_create_error,
    not_found_or_internal,
};

/// Broadcast a task update via SSE so the frontend refreshes.
pub(crate) async fn broadcast_task_update(state: &AppState, id: i64) {
    let updated = {
        let store = state.task_store.read().await;
        match store.find_by_id(id).await {
            Ok(Some(task)) => Some(serde_json::to_value(&task).unwrap()),
            Ok(None) => {
                tracing::warn!(task_id = id, "broadcast skipped -- task not found");
                return;
            }
            Err(e) => {
                tracing::warn!(task_id = id, error = %e, "broadcast skipped -- DB read failed");
                return;
            }
        }
    };
    state.bus.send(
        global_types::BusEvent::Tasks,
        Some(json!({"action": "updated", "item": updated, "id": id})),
    );
}

/// Bump a workbench's `last_activity_at` and broadcast the update via SSE.
/// No-op if `workbench_id` is 0 (no workbench linked).
pub(crate) async fn touch_workbench_activity(state: &AppState, workbench_id: i64) {
    if workbench_id == 0 {
        return;
    }
    let pool = state.db.pool();
    let touched = match captain::io::queries::workbenches::touch_activity(pool, workbench_id).await
    {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!(workbench_id, error = %e, "failed to touch workbench activity");
            return;
        }
    };
    if touched {
        match captain::io::queries::workbenches::find_by_id(pool, workbench_id).await {
            Ok(Some(updated)) => {
                state.bus.send(
                    global_types::BusEvent::Workbenches,
                    Some(json!({"action": "updated", "item": updated})),
                );
            }
            Ok(None) => {
                tracing::warn!(workbench_id, "workbench not found after activity touch");
            }
            Err(e) => {
                tracing::warn!(workbench_id, error = %e, "failed to load workbench after touch");
            }
        }
    }
}

/// Resolve a task's working directory for CC sessions (advisor, ask).
pub(crate) fn resolve_task_cwd(
    item: &captain::Task,
    state: &AppState,
) -> Result<std::path::PathBuf, (StatusCode, Json<Value>)> {
    item.worktree
        .as_deref()
        .map(global_infra::paths::expand_tilde)
        .filter(|p| p.is_dir())
        .or_else(|| {
            let cfg = state.config.load_full();
            settings::config::paths::first_project_path(&cfg)
                .map(|p| global_infra::paths::expand_tilde(&p))
                .filter(|p| p.is_dir())
        })
        .ok_or_else(|| {
            error_response(
                StatusCode::BAD_REQUEST,
                "no worktree or project configured -- cannot run session",
            )
        })
}
