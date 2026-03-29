//! POST /api/client-logs — accepts log entries from Electron main/renderer.
//!
//! The Electron client batches log entries and ships them to the daemon.
//! Each entry is re-emitted as a server-side tracing event with
//! `module = "electron"`, unifying client and server logs into a single
//! structured pipeline.

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;

use crate::AppState;

#[derive(Deserialize)]
pub(crate) struct ClientLogEntry {
    pub level: String,
    pub message: String,
    #[serde(default)]
    pub context: Option<serde_json::Value>,
    #[serde(default)]
    pub timestamp: Option<String>,
}

#[derive(Deserialize)]
pub(crate) struct ClientLogBatch {
    pub entries: Vec<ClientLogEntry>,
}

const MAX_BATCH_SIZE: usize = 500;

pub(crate) async fn post_client_logs(
    State(_state): State<AppState>,
    Json(batch): Json<ClientLogBatch>,
) -> (StatusCode, Json<serde_json::Value>) {
    if batch.entries.len() > MAX_BATCH_SIZE {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({
                "error": format!("batch too large (max {MAX_BATCH_SIZE})")
            })),
        );
    }

    for entry in &batch.entries {
        let ts = entry.timestamp.as_deref().unwrap_or("");
        match entry.level.as_str() {
            "error" => tracing::error!(
                module = "electron",
                client_ts = ts,
                context = ?entry.context,
                "{}", entry.message
            ),
            "warn" => tracing::warn!(
                module = "electron",
                client_ts = ts,
                context = ?entry.context,
                "{}", entry.message
            ),
            "debug" => tracing::debug!(
                module = "electron",
                client_ts = ts,
                context = ?entry.context,
                "{}", entry.message
            ),
            _ => tracing::info!(
                module = "electron",
                client_ts = ts,
                context = ?entry.context,
                "{}", entry.message
            ),
        }
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "accepted": batch.entries.len()
        })),
    )
}
