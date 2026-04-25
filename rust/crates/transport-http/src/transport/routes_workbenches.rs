//! /api/workbenches/* route handlers.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;

use crate::response::{error_response, internal_error, ApiError};
use crate::{ApiRouter, AppState};

pub(crate) fn routes() -> ApiRouter<AppState> {
    let router = ApiRouter::new();
    let router = crate::api_route!(
        router,
        GET "/api/workbenches",
        transport = Json,
        auth = Protected,
        handler = get_workbenches,
        query = api_types::WorkbenchListQuery,
        res = api_types::WorkbenchesResponse
    );
    crate::api_route!(
        router,
        PATCH "/api/workbenches/{id}",
        transport = Json,
        auth = Protected,
        handler = patch_workbench,
        body = api_types::WorkbenchPatchRequest,
        params = api_types::WorkbenchIdParams,
        res = api_types::WorkbenchItem
    )
}

// ── GET /api/workbenches ───────────────────────────────────────────────

fn wire_workbench(workbench: impl serde::Serialize) -> Result<api_types::WorkbenchItem, ApiError> {
    serde_json::from_value(
        serde_json::to_value(workbench)
            .map_err(|e| internal_error(e, "failed to serialize workbench"))?,
    )
    .map_err(|e| internal_error(e, "failed to convert workbench to api type"))
}

#[crate::instrument_api(method = "GET", path = "/api/workbenches")]
pub(crate) async fn get_workbenches(
    State(state): State<AppState>,
    Query(query): Query<api_types::WorkbenchListQuery>,
) -> Result<Json<api_types::WorkbenchesResponse>, ApiError> {
    let status = query
        .status
        .unwrap_or(api_types::WorkbenchStatusFilter::Active)
        .as_str();
    let items = state
        .captain
        .list_workbenches(status)
        .await
        .map_err(|e| internal_error(e, "failed to load workbenches"))?;
    let workbenches = items
        .into_iter()
        .map(wire_workbench)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Json(api_types::WorkbenchesResponse { workbenches }))
}

// ── PATCH /api/workbenches/:id ─────────────────────────────────────────

#[crate::instrument_api(method = "PATCH", path = "/api/workbenches/{id}")]
pub(crate) async fn patch_workbench(
    State(state): State<AppState>,
    Path(api_types::WorkbenchIdParams { id }): Path<api_types::WorkbenchIdParams>,
    Json(body): Json<api_types::WorkbenchPatchRequest>,
) -> Result<Json<api_types::WorkbenchItem>, ApiError> {
    let outcome = state
        .captain
        .patch_workbench(
            id,
            captain::WorkbenchPatch {
                title: body.title,
                archived: body.archived,
                pinned: body.pinned,
            },
        )
        .await
        .map_err(|e| internal_error(e, "failed to update workbench"))?;

    match outcome {
        captain::WorkbenchPatchOutcome::Updated(updated) => Ok(Json(wire_workbench(updated)?)),
        captain::WorkbenchPatchOutcome::NotFound => {
            Err(error_response(StatusCode::NOT_FOUND, "workbench not found"))
        }
        captain::WorkbenchPatchOutcome::Conflict(message) => {
            Err(error_response(StatusCode::CONFLICT, &message))
        }
    }
}
