//! Shared HTTP response helpers for route handlers.

use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

pub(crate) fn error_response(status: StatusCode, msg: &str) -> (StatusCode, Json<Value>) {
    (status, Json(json!({"error": msg})))
}

/// Shorthand: convert any error into a 500 response.
pub(crate) fn internal_error(e: impl std::fmt::Display) -> (StatusCode, Json<Value>) {
    error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string())
}
