//! /api/analytics route handler.

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde_json::Value;

use crate::response::{error_response, internal_error};
use crate::AppState;

/// GET /api/analytics — aggregated cost, throughput, and success metrics.
pub(crate) async fn get_analytics(
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if !state.config.load().features.analytics {
        return Err(error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "analytics is disabled",
        ));
    }

    let data = mando_db::queries::analytics::fetch_analytics(state.db.pool())
        .await
        .map_err(internal_error)?;

    let value = serde_json::to_value(data).map_err(internal_error)?;
    Ok(Json(value))
}
