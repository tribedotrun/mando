//! POST /api/client-logs — accepts log entries from Electron main/renderer.
//!
//! The Electron client batches log entries and ships them to the daemon.
//! Each entry is re-emitted as a server-side tracing event with
//! `module = "electron"`, unifying client and server logs into a single
//! structured pipeline.

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;

use crate::AppState;

const MAX_BATCH_SIZE: usize = 500;

pub(crate) async fn post_client_logs(
    State(_state): State<AppState>,
    Json(batch): Json<api_types::ClientLogBatchRequest>,
) -> Result<Json<api_types::ClientLogBatchResponse>, (StatusCode, Json<api_types::ErrorResponse>)> {
    if batch.entries.len() > MAX_BATCH_SIZE {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(api_types::ErrorResponse {
                error: format!("batch too large (max {MAX_BATCH_SIZE})"),
            }),
        ));
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

    Ok(Json(api_types::ClientLogBatchResponse {
        accepted: batch.entries.len(),
    }))
}
