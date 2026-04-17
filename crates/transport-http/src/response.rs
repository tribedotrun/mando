//! Stateless HTTP response helpers for route handlers.
//!
//! Error leak protection: helpers log the raw error (for operator debugging)
//! and return a sanitized generic message to the HTTP client.

use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

/// Build a JSON error response. Use this for user-facing messages (BAD_REQUEST,
/// NOT_FOUND, etc.) where the message is already safe to return as-is.
pub fn error_response(status: StatusCode, msg: &str) -> (StatusCode, Json<Value>) {
    (status, Json(json!({"error": msg})))
}

/// Sanitize an error for INTERNAL_SERVER_ERROR: log the raw form, return
/// an operation-specific client-safe message.
pub fn internal_error(e: impl std::fmt::Display, msg: &str) -> (StatusCode, Json<Value>) {
    let raw = e.to_string();
    tracing::error!(error = %raw, client_msg = msg, "internal error returned to client");
    error_response(StatusCode::INTERNAL_SERVER_ERROR, msg)
}

/// Build an error response with a sanitized client message but log the raw
/// error internally for operator debugging.
pub fn internal_error_with(
    status: StatusCode,
    e: impl std::fmt::Display,
    client_msg: &str,
) -> (StatusCode, Json<Value>) {
    tracing::error!(status = %status, error = %e, client_msg = client_msg, "sanitized error returned to client");
    error_response(status, client_msg)
}

/// Map a task-creation error: project-related bails become 422, everything
/// else becomes a sanitized 500.
pub fn map_task_create_error(e: anyhow::Error) -> (StatusCode, Json<Value>) {
    let msg = e.to_string();
    if msg.contains("no project configured") || msg.contains("project selection required") {
        error_response(StatusCode::UNPROCESSABLE_ENTITY, &msg)
    } else {
        internal_error(e, "failed to create task")
    }
}

/// Map an error to 404 if it looks like "not found", else 500.
pub fn not_found_or_internal(
    e: impl std::fmt::Display,
    context: &str,
) -> (StatusCode, Json<Value>) {
    let raw = e.to_string();
    if raw.to_lowercase().contains("not found") {
        tracing::debug!(error = %raw, "resource not found");
        error_response(StatusCode::NOT_FOUND, "not found")
    } else {
        tracing::error!(error = %raw, client_msg = context, "internal error returned to client");
        error_response(StatusCode::INTERNAL_SERVER_ERROR, context)
    }
}
