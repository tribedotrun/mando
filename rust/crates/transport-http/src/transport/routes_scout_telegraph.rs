//! /api/scout/items/{id}/telegraph route handler.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use scout::find_scout_error;

use crate::response::{error_response, internal_error, ApiError};
use crate::AppState;

/// POST /api/scout/items/{id}/telegraph — publish article to Telegraph, return URL.
#[crate::instrument_api(method = "POST", path = "/api/scout/items/{id}/telegraph")]
pub(crate) async fn publish_telegraph(
    State(state): State<AppState>,
    Path(api_types::ScoutItemIdParams { id }): Path<api_types::ScoutItemIdParams>,
    Json(_body): Json<api_types::EmptyRequest>,
) -> Result<Json<api_types::TelegraphPublishResponse>, ApiError> {
    let url = state.scout.publish_telegraph(id).await.map_err(|e| {
        if let Some(typed) = find_scout_error(&e) {
            let status = if typed.is_not_found() {
                StatusCode::NOT_FOUND
            } else if typed.is_client_error() {
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            return error_response(status, &typed.to_string());
        }
        internal_error(e, "failed to publish to Telegraph")
    })?;

    Ok(Json(api_types::TelegraphPublishResponse { ok: true, url }))
}
