//! /api/scout/bulk* route handlers.

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

use crate::response::error_response;
use crate::AppState;

#[derive(serde::Deserialize)]
pub(crate) struct BulkUpdateBody {
    pub ids: Vec<i64>,
    pub updates: BulkUpdates,
}

#[derive(serde::Deserialize)]
pub(crate) struct BulkUpdates {
    pub status: String,
}

/// POST /api/scout/bulk — update status for multiple items.
pub(crate) async fn post_scout_bulk_update(
    State(state): State<AppState>,
    Json(body): Json<BulkUpdateBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let pool = state.db.pool();
    let mut updated = 0u32;
    for id in &body.ids {
        if let Err(e) = mando_scout::update_scout_status(pool, *id, &body.updates.status).await {
            return Err(error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &e.to_string(),
            ));
        }
        updated += 1;
    }
    state.bus.send(mando_types::BusEvent::Scout, None);
    Ok(Json(json!({"updated": updated})))
}

#[derive(serde::Deserialize)]
pub(crate) struct BulkDeleteBody {
    pub ids: Vec<i64>,
}

/// POST /api/scout/bulk-delete — delete multiple items.
pub(crate) async fn post_scout_bulk_delete(
    State(state): State<AppState>,
    Json(body): Json<BulkDeleteBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let pool = state.db.pool();
    let mut deleted = 0u32;
    for id in &body.ids {
        if let Err(e) = mando_scout::delete_scout_item(pool, *id).await {
            return Err(error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &e.to_string(),
            ));
        }
        deleted += 1;
    }
    state.bus.send(mando_types::BusEvent::Scout, None);
    Ok(Json(json!({"deleted": deleted})))
}
