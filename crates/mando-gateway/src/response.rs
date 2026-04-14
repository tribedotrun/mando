//! Shared HTTP response helpers for route handlers.
//!
//! Error leak protection: helpers log the raw error (for operator debugging)
//! and return a sanitized generic message to the HTTP client. Raw internal
//! errors (SQL, file paths, library internals) must never appear in response
//! bodies — they can expose schema, config layout, or dependency versions.

use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

use crate::AppState;

/// Build a JSON error response. Use this for user-facing messages (BAD_REQUEST,
/// NOT_FOUND, etc.) where the message is already safe to return as-is.
pub(crate) fn error_response(status: StatusCode, msg: &str) -> (StatusCode, Json<Value>) {
    (status, Json(json!({"error": msg})))
}

/// Sanitize an error for INTERNAL_SERVER_ERROR: log the raw form, return
/// a generic client-safe message. Use via `.map_err(internal_error)?`.
pub(crate) fn internal_error(e: impl std::fmt::Display) -> (StatusCode, Json<Value>) {
    let raw = e.to_string();
    tracing::error!(error = %raw, "internal error returned to client");
    error_response(StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
}

/// Build an error response with a sanitized client message but log the raw
/// error internally for operator debugging. Use this at direct call sites
/// that previously did `error_response(status, &e.to_string())` for server
/// (5xx) errors.
pub(crate) fn internal_error_with(
    status: StatusCode,
    e: impl std::fmt::Display,
    client_msg: &str,
) -> (StatusCode, Json<Value>) {
    tracing::error!(status = %status, error = %e, client_msg = client_msg, "sanitized error returned to client");
    error_response(status, client_msg)
}

/// Map a task-creation error: project-related bails become 422, everything
/// else becomes a sanitized 500. Used by both `/api/tasks/add` and scout promote.
pub(crate) fn map_task_create_error(e: anyhow::Error) -> (StatusCode, Json<Value>) {
    let msg = e.to_string();
    if msg.contains("no project configured") || msg.contains("project selection required") {
        error_response(StatusCode::UNPROCESSABLE_ENTITY, &msg)
    } else {
        internal_error(e)
    }
}

/// Map an error to 404 if it represents a "record not found" condition,
/// else 500. The raw error is always logged and only a sanitized message is
/// returned to the client.
///
/// The heuristic matches the literal substring `"not found"` so common
/// gateway error shapes all map to 404:
///   - `"not found"`                       (bare)
///   - `"task not found: 42"`              (prefixed + id suffix)
///   - `"stream not found for session X"`  (prefixed + trailing context)
///   - `"record not found"`                (repository layer)
///
/// The simple `contains` check was intentional after an earlier, stricter
/// variant regressed these cases. False positives on errors that embed the
/// phrase as context text (e.g. `"failed to load PR, comment not found in
/// cache"`) are accepted because the alternative is dropping legitimate
/// 404s for the much more common shapes above.
pub(crate) fn not_found_or_internal(e: impl std::fmt::Display) -> (StatusCode, Json<Value>) {
    let raw = e.to_string();
    if raw.to_lowercase().contains("not found") {
        tracing::debug!(error = %raw, "resource not found");
        error_response(StatusCode::NOT_FOUND, "not found")
    } else {
        tracing::error!(error = %raw, "internal error returned to client");
        error_response(StatusCode::INTERNAL_SERVER_ERROR, "internal server error")
    }
}

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
        mando_types::BusEvent::Tasks,
        Some(json!({"action": "updated", "item": updated, "id": id})),
    );
}

/// Resolve a task's working directory for CC sessions (advisor, ask).
pub(crate) fn resolve_task_cwd(
    item: &mando_types::Task,
    state: &AppState,
) -> Result<std::path::PathBuf, (StatusCode, Json<Value>)> {
    item.worktree
        .as_deref()
        .map(mando_config::expand_tilde)
        .filter(|p| p.is_dir())
        .or_else(|| {
            let cfg = state.config.load_full();
            mando_config::paths::first_project_path(&cfg)
                .map(|p| mando_config::paths::expand_tilde(&p))
                .filter(|p| p.is_dir())
        })
        .ok_or_else(|| {
            error_response(
                StatusCode::BAD_REQUEST,
                "no worktree or project configured -- cannot run session",
            )
        })
}
