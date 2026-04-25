//! HTTP response helpers for route handlers.
//!
//! Stateless helpers return sanitized envelopes.
//! Stateful helpers broadcast task and workbench updates through the HTTP state.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

use crate::AppState;

/// Typed error envelope per wire contract. Handlers return
/// `Result<Json<T>, ApiError>`; the error path carries a strict
/// `api_types::ErrorResponse` body, never `serde_json::Value`.
pub type ApiError = (StatusCode, Json<api_types::ErrorResponse>);

/// Response wrapper for 201 Created routes. Replaces the bare
/// `(StatusCode::CREATED, Json<T>)` tuple so every 201 handler has a named
/// return shape (`Result<ApiCreated<T>, ApiError>`). Serializes `T` as JSON.
pub struct ApiCreated<T: Serialize>(pub T);

impl<T: Serialize> IntoResponse for ApiCreated<T> {
    fn into_response(self) -> Response {
        (StatusCode::CREATED, Json(self.0)).into_response()
    }
}

/// Response wrapper for 204 No Content routes. No body is serialized.
/// Pair with `api_types::EmptyResponse` in the `res = ...` declaration when
/// the route is documented as returning an empty envelope instead of 204.
#[allow(dead_code)]
pub struct ApiNoContent;

impl IntoResponse for ApiNoContent {
    fn into_response(self) -> Response {
        StatusCode::NO_CONTENT.into_response()
    }
}

/// Build a JSON error response. Use this for user-facing messages (BAD_REQUEST,
/// NOT_FOUND, etc.) where the message is already safe to return as-is.
pub fn error_response(status: StatusCode, msg: &str) -> ApiError {
    (
        status,
        Json(api_types::ErrorResponse {
            error: msg.to_string(),
        }),
    )
}

/// Sanitize an error for INTERNAL_SERVER_ERROR: log the raw form, return
/// an operation-specific client-safe message. When the error carries an
/// anyhow-style chain, the alternate Display formatter (`{:#}`) expands
/// it so downstream operators see *why*, not just the outermost context.
pub fn internal_error(e: impl std::fmt::Display, msg: &str) -> ApiError {
    let raw = format!("{e:#}");
    tracing::error!(module = "transport-http-response", error = %raw, client_msg = msg, "internal error returned to client");
    error_response(StatusCode::INTERNAL_SERVER_ERROR, msg)
}

/// Build an error response with a sanitized client message but log the raw
/// error internally for operator debugging.
pub fn internal_error_with(
    status: StatusCode,
    e: impl std::fmt::Display,
    client_msg: &str,
) -> ApiError {
    tracing::error!(module = "transport-http-response", status = %status, error = %e, client_msg = client_msg, "sanitized error returned to client");
    error_response(status, client_msg)
}

/// Map a task-creation error: project-related failures become 422,
/// everything else becomes a sanitized 500. Matches on typed
/// [`captain::TaskCreateError`] variants walked through the anyhow chain.
pub fn map_task_create_error(e: anyhow::Error) -> ApiError {
    if let Some(typed) = captain::find_task_create_error(&e) {
        return error_response(StatusCode::UNPROCESSABLE_ENTITY, &typed.to_string());
    }
    internal_error(e, "failed to create task")
}

pub async fn broadcast_task_update(state: &AppState, id: i64) {
    state.captain.broadcast_task_update(id).await;
}

pub async fn touch_workbench_activity(state: &AppState, workbench_id: i64) {
    state.captain.touch_workbench_activity(workbench_id).await;
}

pub fn resolve_task_cwd(
    item: &captain::Task,
    state: &AppState,
) -> Result<std::path::PathBuf, ApiError> {
    // Surface the captain's error text verbatim — it already distinguishes
    // "no worktree assigned" from "worktree missing on disk" so the user
    // sees actionable detail (reopen the task, etc.) instead of a generic
    // message that masks the real state.
    state
        .captain
        .resolve_task_cwd(item)
        .map_err(|e| error_response(StatusCode::CONFLICT, &format!("{e}")))
}
