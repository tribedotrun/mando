//! /api/scout/* route handlers.

use std::panic::AssertUnwindSafe;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use futures_util::FutureExt;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::response::{
    error_response, internal_error, map_task_create_error, not_found_or_internal,
};
use crate::scout_notify::{emit_scout_process_failed, emit_scout_processed};
use crate::AppState;

/// Spawn background processing for a newly added scout item.
///
/// Mirrors the inline spawn in `post_scout_items` but extracted so
/// `post_scout_research` can reuse the same pattern.
pub(crate) fn spawn_scout_processing(state: &AppState, id: i64, url: String) {
    let config = state.config.load_full();
    let workflow = state.scout_workflow.load_full();
    let pool = state.db.pool().clone();
    let bus = state.bus.clone();
    state.task_tracker.spawn(async move {
        let result = AssertUnwindSafe(async {
            if let Err(e) = mando_scout::process_scout(&config, &pool, Some(id), &workflow).await {
                tracing::warn!(scout_id = id, error = %e, "auto-process failed");
                emit_scout_process_failed(&bus, id, &url, &e.to_string());
                return;
            }
            let scout_payload = mando_scout::get_scout_item(&pool, id).await.ok();
            bus.send(
                mando_types::BusEvent::Scout,
                Some(json!({"action": "updated", "item": scout_payload, "id": id})),
            );
            emit_scout_processed(&bus, &pool, id).await;
        })
        .catch_unwind()
        .await;
        if let Err(panic) = result {
            tracing::error!(scout_id = id, ?panic, "auto-process panicked");
        }
    });
}

#[derive(Deserialize, Default)]
pub(crate) struct ScoutQuery {
    pub status: Option<String>,
    pub q: Option<String>,
    #[serde(rename = "type")]
    pub item_type: Option<String>,
    pub page: Option<usize>,
    pub per_page: Option<usize>,
}

// ---------------------------------------------------------------
// GET endpoints
// ---------------------------------------------------------------

/// GET /api/scout/items?status=pending
pub(crate) async fn get_scout_items(
    State(state): State<AppState>,
    Query(params): Query<ScoutQuery>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let pool = state.db.pool();
    match mando_scout::list_scout_items(
        pool,
        params.status.as_deref(),
        params.q.as_deref(),
        params.item_type.as_deref(),
        params.page,
        params.per_page,
    )
    .await
    {
        Ok(val) => Ok(Json(val)),
        Err(e) => Err(error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &e.to_string(),
        )),
    }
}

/// GET /api/scout/items/{id}
pub(crate) async fn get_scout_item(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let pool = state.db.pool();
    mando_scout::get_scout_item(pool, id)
        .await
        .map(Json)
        .map_err(not_found_or_internal)
}

/// GET /api/scout/items/{id}/article
pub(crate) async fn get_scout_article(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let pool = state.db.pool();
    let workflow = state.scout_workflow.load_full();
    mando_scout::ensure_scout_article(pool, id, &workflow)
        .await
        .map(Json)
        .map_err(not_found_or_internal)
}

#[derive(Deserialize)]
pub(crate) struct AddScoutBody {
    pub url: String,
    pub title: Option<String>,
}

/// POST /api/scout/items
pub(crate) async fn post_scout_items(
    State(state): State<AppState>,
    Json(body): Json<AddScoutBody>,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<Value>)> {
    if !body.url.starts_with("http://") && !body.url.starts_with("https://") {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "URL must start with http:// or https://",
        ));
    }
    let pool = state.db.pool();
    match mando_scout::add_scout_item(pool, &body.url, body.title.as_deref()).await {
        Ok(val) => {
            let scout_payload = if let Some(id) = val["id"].as_i64() {
                mando_scout::get_scout_item(pool, id).await.ok()
            } else {
                None
            };
            state.bus.send(
                mando_types::BusEvent::Scout,
                Some(json!({"action": "created", "item": scout_payload, "id": val["id"]})),
            );

            if val["added"].as_bool() == Some(true) {
                if let Some(id) = val["id"].as_i64() {
                    spawn_scout_processing(&state, id, body.url.clone());
                }
            }

            Ok((StatusCode::CREATED, Json(val)))
        }
        Err(e) => Err(error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &e.to_string(),
        )),
    }
}

#[derive(Deserialize)]
pub(crate) struct ProcessBody {
    pub id: Option<i64>,
}

/// POST /api/scout/process
pub(crate) async fn post_scout_process(
    State(state): State<AppState>,
    Json(body): Json<ProcessBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let config = state.config.load_full();
    let workflow = state.scout_workflow.load_full();
    let pool = state.db.pool();

    let val = mando_scout::process_scout(&config, pool, body.id, &workflow)
        .await
        .map_err(internal_error)?;

    if let Some(id) = body.id {
        let scout_payload = mando_scout::get_scout_item(pool, id).await.ok();
        state.bus.send(
            mando_types::BusEvent::Scout,
            Some(json!({"action": "updated", "item": scout_payload, "id": id})),
        );
        emit_scout_processed(&state.bus, state.db.pool(), id).await;
    } else {
        state.bus.send(
            mando_types::BusEvent::Scout,
            Some(json!({"action": "updated"})),
        );
    }

    Ok(Json(val))
}

#[derive(Deserialize)]
pub(crate) struct ActBody {
    pub project: String,
    pub prompt: Option<String>,
}

/// POST /api/scout/items/{id}/act — generate a task from a scout item.
pub(crate) async fn post_scout_act(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<ActBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let config = state.config.load_full();
    let workflow = state.scout_workflow.load_full();
    let pool = state.db.pool();

    let ai_result = mando_scout::act_on_scout_item(
        &config,
        pool,
        id,
        &body.project,
        body.prompt.as_deref(),
        &workflow,
    )
    .await
    .map_err(|e| {
        let msg = e.to_string();
        if msg.contains("unknown project") {
            error_response(StatusCode::BAD_REQUEST, &msg)
        } else if msg.contains("not found") {
            error_response(StatusCode::NOT_FOUND, "not found")
        } else {
            crate::response::internal_error(e)
        }
    })?;

    if ai_result["skipped"].as_bool() == Some(true) {
        return Ok(Json(ai_result));
    }

    let task_title = ai_result["task_title"].as_str().ok_or_else(|| {
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "AI response missing task_title",
        )
    })?;
    let task_description = ai_result["task_description"].as_str();
    let project_name = ai_result["project"].as_str();

    let config = state.config.load_full();
    let store = state.task_store.read().await;
    let val = mando_captain::runtime::dashboard::add_task_with_context(
        &config,
        &store,
        task_title,
        project_name,
        task_description,
        Some("scout"),
    )
    .await
    .map_err(map_task_create_error)?;

    let task_payload = if let Some(id) = val["id"].as_i64() {
        store
            .find_by_id(id)
            .await
            .ok()
            .flatten()
            .map(|t| serde_json::to_value(&t).unwrap())
    } else {
        None
    };
    state.bus.send(
        mando_types::BusEvent::Tasks,
        Some(serde_json::json!({"action": "created", "item": task_payload, "id": val["id"]})),
    );

    Ok(Json(serde_json::json!({
        "ok": true,
        "task_id": val["id"],
        "title": val["title"],
    })))
}

// ---------------------------------------------------------------
// PATCH endpoint
// ---------------------------------------------------------------

#[derive(Deserialize)]
pub(crate) struct PatchScoutBody {
    pub status: String,
}

/// PATCH /api/scout/items/{id}
pub(crate) async fn patch_scout_item(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<PatchScoutBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let pool = state.db.pool();
    mando_scout::update_scout_status(pool, id, &body.status)
        .await
        .map_err(|e| {
            let msg = e.to_string();
            let status = if msg.contains("invalid status") {
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            error_response(status, &msg)
        })?;
    let scout_payload = mando_scout::get_scout_item(pool, id).await.ok();
    state.bus.send(
        mando_types::BusEvent::Scout,
        Some(json!({"action": "updated", "item": scout_payload, "id": id})),
    );
    Ok(Json(json!({"ok": true})))
}

// ---------------------------------------------------------------
// DELETE endpoint
// ---------------------------------------------------------------

/// DELETE /api/scout/items/{id}
pub(crate) async fn delete_scout_item(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let pool = state.db.pool();
    let val = mando_scout::delete_scout_item(pool, id)
        .await
        .map_err(not_found_or_internal)?;
    state.bus.send(
        mando_types::BusEvent::Scout,
        Some(json!({"action": "deleted", "id": id})),
    );
    Ok(Json(val))
}

/// GET /api/scout/items/{id}/sessions — list CC sessions for a scout item.
pub(crate) async fn get_scout_item_sessions(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let pool = state.db.pool();
    let sessions = mando_db::queries::sessions::list_sessions_for_scout_item(pool, id)
        .await
        .map_err(internal_error)?;
    Ok(Json(json!(sessions)))
}
