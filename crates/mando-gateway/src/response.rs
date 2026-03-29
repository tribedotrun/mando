//! Shared HTTP response helpers for route handlers.

use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

pub(crate) fn error_response(status: StatusCode, msg: &str) -> (StatusCode, Json<Value>) {
    (status, Json(json!({"error": msg})))
}
