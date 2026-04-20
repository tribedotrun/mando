//! /api/stats/* route handlers.

use axum::extract::State;
use axum::Json;

use crate::response::internal_error;
use crate::response::ApiError;
use crate::AppState;

pub(crate) async fn get_activity_stats(
    State(state): State<AppState>,
) -> Result<Json<api_types::ActivityStatsResponse>, ApiError> {
    let (merged_7d, rows) = state
        .captain
        .activity_stats(56)
        .await
        .map_err(|e| internal_error(e, "failed to load merge stats"))?;
    let daily_merges = rows
        .into_iter()
        .map(|(date, count)| api_types::DailyMerge { date, count })
        .collect();

    Ok(Json(api_types::ActivityStatsResponse {
        merged_7d,
        daily_merges,
    }))
}
