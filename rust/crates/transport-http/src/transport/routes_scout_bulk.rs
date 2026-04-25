//! /api/scout/bulk* route handlers.

use axum::extract::State;
use axum::Json;

use crate::AppState;

fn scout_item_command(command: api_types::ScoutItemLifecycleCommand) -> scout::ScoutItemCommand {
    match command {
        api_types::ScoutItemLifecycleCommand::MarkPending => scout::ScoutItemCommand::MarkPending,
        api_types::ScoutItemLifecycleCommand::MarkProcessed => {
            scout::ScoutItemCommand::MarkProcessed
        }
        api_types::ScoutItemLifecycleCommand::Save => scout::ScoutItemCommand::Save,
        api_types::ScoutItemLifecycleCommand::Archive => scout::ScoutItemCommand::Archive,
    }
}

/// POST /api/scout/bulk — update status for multiple items.
///
/// Per-item failures are collected rather than short-circuited so the
/// response reports how many items succeeded and which ids failed, letting
/// the UI surface partial progress instead of the whole request appearing
/// to fail after the first bad id.
#[crate::instrument_api(method = "POST", path = "/api/scout/bulk")]
pub(crate) async fn post_scout_bulk_update(
    State(state): State<AppState>,
    Json(body): Json<api_types::ScoutBulkCommandRequest>,
) -> Json<api_types::ScoutBulkUpdateResponse> {
    let result = state
        .scout
        .bulk_apply_item_command(&body.ids, scout_item_command(body.command))
        .await;
    if result.updated > 0 {
        state.bus.send(global_bus::BusPayload::Scout(None));
    }
    Json(result)
}

/// POST /api/scout/bulk-delete — delete multiple items.
///
/// Per-item failures are reported alongside the success count (see
/// post_scout_bulk_update for rationale).
#[crate::instrument_api(method = "POST", path = "/api/scout/bulk-delete")]
pub(crate) async fn post_scout_bulk_delete(
    State(state): State<AppState>,
    Json(body): Json<api_types::ScoutBulkDeleteRequest>,
) -> Json<api_types::ScoutBulkDeleteResponse> {
    let result = state.scout.bulk_delete_items(&body.ids).await;
    if result.deleted > 0 {
        state.bus.send(global_bus::BusPayload::Scout(None));
    }
    Json(result)
}
