//! /api/scout/bulk* route handlers.

use axum::extract::State;
use axum::Json;
use serde_json::{json, Value};

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
///
/// Per-item failures are collected rather than short-circuited so the
/// response reports how many items succeeded and which ids failed, letting
/// the UI surface partial progress instead of the whole request appearing
/// to fail after the first bad id.
pub(crate) async fn post_scout_bulk_update(
    State(state): State<AppState>,
    Json(body): Json<BulkUpdateBody>,
) -> Json<Value> {
    let pool = state.db.pool();
    let mut updated = 0u32;
    let mut failed: Vec<Value> = Vec::new();
    for id in &body.ids {
        if let Err(e) = mando_scout::update_scout_status(pool, *id, &body.updates.status).await {
            failed.push(json!({"id": id, "error": e.to_string()}));
        } else {
            updated += 1;
        }
    }
    if updated > 0 {
        state.bus.send(mando_types::BusEvent::Scout, None);
    }
    let status = if !failed.is_empty() && updated == 0 {
        "error"
    } else if !failed.is_empty() {
        "partial"
    } else {
        "ok"
    };
    Json(json!({"updated": updated, "failed": failed, "status": status}))
}

#[derive(serde::Deserialize)]
pub(crate) struct BulkDeleteBody {
    pub ids: Vec<i64>,
}

/// POST /api/scout/bulk-delete — delete multiple items.
///
/// Per-item failures are reported alongside the success count (see
/// post_scout_bulk_update for rationale).
pub(crate) async fn post_scout_bulk_delete(
    State(state): State<AppState>,
    Json(body): Json<BulkDeleteBody>,
) -> Json<Value> {
    let pool = state.db.pool();
    let mut deleted = 0u32;
    let mut failed: Vec<Value> = Vec::new();
    for id in &body.ids {
        if let Err(e) = mando_scout::delete_scout_item(pool, *id).await {
            failed.push(json!({"id": id, "error": e.to_string()}));
        } else {
            deleted += 1;
        }
    }
    if deleted > 0 {
        state.bus.send(mando_types::BusEvent::Scout, None);
    }
    let status = if !failed.is_empty() && deleted == 0 {
        "error"
    } else if !failed.is_empty() {
        "partial"
    } else {
        "ok"
    };
    Json(json!({"deleted": deleted, "failed": failed, "status": status}))
}
