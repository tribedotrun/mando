//! /api/stats/* route handlers.

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::Serialize;
use serde_json::Value;

use crate::response::internal_error;
use crate::AppState;

#[derive(Serialize)]
pub(crate) struct DailyMerge {
    date: String,
    count: i64,
}

#[derive(Serialize)]
pub(crate) struct ActivityStatsResponse {
    merged_7d: i64,
    daily_merges: Vec<DailyMerge>,
}

pub(crate) async fn get_activity_stats(
    State(state): State<AppState>,
) -> Result<Json<ActivityStatsResponse>, (StatusCode, Json<Value>)> {
    let store = state.task_store.read().await;
    let rows = store.daily_merge_counts(56).await.map_err(internal_error)?;

    let today = time::OffsetDateTime::now_utc().date();
    let cutoff_7d = today - time::Duration::days(7);

    let mut merged_7d: i64 = 0;
    let mut daily_merges = Vec::with_capacity(rows.len());

    for (date_str, count) in rows {
        if let Ok(d) = time::Date::parse(
            &date_str,
            &time::format_description::well_known::Iso8601::DATE,
        ) {
            if d >= cutoff_7d {
                merged_7d += count;
            }
        }
        daily_merges.push(DailyMerge {
            date: date_str,
            count,
        });
    }

    Ok(Json(ActivityStatsResponse {
        merged_7d,
        daily_merges,
    }))
}
