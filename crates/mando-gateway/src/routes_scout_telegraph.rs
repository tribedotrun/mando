//! /api/scout/items/{id}/telegraph route handler.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde_json::Value;

use crate::response::{error_response, internal_error};
use crate::AppState;

/// POST /api/scout/items/{id}/telegraph — publish article to Telegraph, return URL.
pub(crate) async fn publish_telegraph(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let pool = state.db.pool();
    let workflow = state.scout_workflow.load_full();
    let article = mando_scout::ensure_scout_article(pool, id, &workflow)
        .await
        .map_err(|e| {
            let msg = e.to_string();
            if msg.contains("not found") {
                error_response(StatusCode::NOT_FOUND, &msg)
            } else {
                error_response(StatusCode::INTERNAL_SERVER_ERROR, &msg)
            }
        })?;

    let title = article["title"].as_str().unwrap_or("Untitled");
    let article_md = article["article"].as_str().ok_or_else(|| {
        error_response(
            StatusCode::NOT_FOUND,
            "no article content — needs processing",
        )
    })?;

    let url = mando_scout::io::telegraph::publish_article(id, title, article_md)
        .await
        .map_err(internal_error)?;

    Ok(Json(serde_json::json!({"ok": true, "url": url})))
}
