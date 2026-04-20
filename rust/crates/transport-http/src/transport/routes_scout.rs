//! /api/scout/* route handlers.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use scout::find_scout_error;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::response::{
    error_response, internal_error, map_task_create_error, ApiCreated, ApiError,
};
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

/// Map a scout runtime error onto the correct HTTP status when the chain
/// carries a typed `ScoutError`; otherwise fall through to `fallback`.
fn map_scout_error(
    err: anyhow::Error,
    fallback: impl FnOnce(anyhow::Error) -> ApiError,
) -> ApiError {
    let Some(typed) = find_scout_error(&err) else {
        return fallback(err);
    };
    let message = typed.to_string();
    let status = if typed.is_not_found() {
        StatusCode::NOT_FOUND
    } else if typed.is_client_error() {
        StatusCode::BAD_REQUEST
    } else {
        StatusCode::INTERNAL_SERVER_ERROR
    };
    error_response(status, &message)
}

// ---------------------------------------------------------------
// GET endpoints
// ---------------------------------------------------------------

fn decode_response<T: DeserializeOwned>(
    value: Value,
    context: &'static str,
) -> Result<T, ApiError> {
    serde_json::from_value(value).map_err(|err| internal_error(err, context))
}

/// GET /api/scout/items?status=pending
#[crate::instrument_api(method = "GET", path = "/api/scout/items")]
pub(crate) async fn get_scout_items(
    State(state): State<AppState>,
    Query(params): Query<api_types::ScoutQuery>,
) -> Result<Json<api_types::ScoutResponse>, ApiError> {
    match state
        .scout
        .list_items(
            params.status.as_deref(),
            params.q.as_deref(),
            params.item_type.as_deref(),
            params.page,
            params.per_page,
        )
        .await
    {
        Ok(val) => Ok(Json(decode_response(
            val,
            "failed to decode scout list response",
        )?)),
        Err(e) => Err(internal_error(e, "failed to list scout items")),
    }
}

/// GET /api/scout/items/{id}
#[crate::instrument_api(method = "GET", path = "/api/scout/items/{id}")]
pub(crate) async fn get_scout_item(
    State(state): State<AppState>,
    Path(api_types::ScoutItemIdParams { id }): Path<api_types::ScoutItemIdParams>,
) -> Result<Json<api_types::ScoutItem>, ApiError> {
    let value = state
        .scout
        .get_item_value(id)
        .await
        .map_err(|e| map_scout_error(e, |e| internal_error(e, "failed to load scout item")))?;
    Ok(Json(decode_response(value, "failed to decode scout item")?))
}

/// GET /api/scout/items/{id}/article
#[crate::instrument_api(method = "GET", path = "/api/scout/items/{id}/article")]
pub(crate) async fn get_scout_article(
    State(state): State<AppState>,
    Path(api_types::ScoutItemIdParams { id }): Path<api_types::ScoutItemIdParams>,
) -> Result<Json<api_types::ScoutArticleResponse>, ApiError> {
    let value =
        state.scout.get_article_value(id).await.map_err(|e| {
            map_scout_error(e, |e| internal_error(e, "failed to load scout article"))
        })?;
    Ok(Json(decode_response(
        value,
        "failed to decode scout article",
    )?))
}

/// POST /api/scout/items
#[crate::instrument_api(method = "POST", path = "/api/scout/items")]
pub(crate) async fn post_scout_items(
    State(state): State<AppState>,
    Json(body): Json<api_types::ScoutAddRequest>,
) -> Result<ApiCreated<api_types::ScoutAddResponse>, ApiError> {
    if !body.url.starts_with("http://") && !body.url.starts_with("https://") {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "URL must start with http:// or https://",
        ));
    }
    match state.scout.add_item(&body.url, body.title.as_deref()).await {
        Ok(val) => {
            let scout_payload = if let Some(id) = val["id"].as_i64() {
                state.scout.get_item_value(id).await.ok()
            } else {
                None
            };
            let scout_id = val["id"].as_i64();
            let scout_item: Option<api_types::ScoutItem> =
                scout_payload.and_then(|v| serde_json::from_value(v).ok());
            state.bus.send(global_bus::BusPayload::Scout(Some(
                api_types::ScoutEventData {
                    action: Some("created".into()),
                    item: scout_item,
                    id: scout_id,
                },
            )));

            if val["added"].as_bool() == Some(true) {
                if let Some(id) = val["id"].as_i64() {
                    state.scout.spawn_processing(id, body.url.clone());
                }
            }

            Ok(ApiCreated(decode_response(
                val,
                "failed to decode scout create response",
            )?))
        }
        Err(e) => Err(internal_error(e, "failed to add scout item")),
    }
}

/// POST /api/scout/process
#[crate::instrument_api(method = "POST", path = "/api/scout/process")]
pub(crate) async fn post_scout_process(
    State(state): State<AppState>,
    Json(body): Json<api_types::ScoutProcessRequest>,
) -> Result<Json<api_types::ProcessResponse>, ApiError> {
    let val = state
        .scout
        .process_item(body.id)
        .await
        .map_err(|e| internal_error(e, "failed to process scout item"))?;

    if let Some(id) = body.id {
        let scout_payload = match state.scout.get_item_value(id).await {
            Ok(v) => Some(v),
            Err(e) => {
                tracing::warn!(module = "transport-http-transport-routes_scout", scout_id = id, error = %e, "failed to fetch scout item for SSE event");
                None
            }
        };
        let scout_item: Option<api_types::ScoutItem> =
            scout_payload.and_then(|v| serde_json::from_value(v).ok());
        state.bus.send(global_bus::BusPayload::Scout(Some(
            api_types::ScoutEventData {
                action: Some("updated".into()),
                item: scout_item,
                id: Some(id),
            },
        )));
        state.scout.emit_processed_notification(id).await;
    } else {
        state.bus.send(global_bus::BusPayload::Scout(Some(
            api_types::ScoutEventData {
                action: Some("updated".into()),
                item: None,
                id: None,
            },
        )));
    }

    Ok(Json(decode_response(
        val,
        "failed to decode scout process response",
    )?))
}

/// POST /api/scout/items/{id}/act — generate a task from a scout item.
#[crate::instrument_api(method = "POST", path = "/api/scout/items/{id}/act")]
pub(crate) async fn post_scout_act(
    State(state): State<AppState>,
    Path(api_types::ScoutItemIdParams { id }): Path<api_types::ScoutItemIdParams>,
    Json(body): Json<api_types::ScoutActRequest>,
) -> Result<Json<api_types::ActResponse>, ApiError> {
    let ai_result = state
        .scout
        .act_on_item(id, &body.project, body.prompt.as_deref())
        .await
        .map_err(|e| map_scout_error(e, |e| internal_error(e, "scout action failed")))?;

    if ai_result["skipped"].as_bool() == Some(true) {
        return Ok(Json(api_types::ActResponse {
            ok: Some(true),
            task_id: None,
            title: None,
            skipped: Some(true),
            reason: ai_result["reason"].as_str().map(str::to_owned),
        }));
    }

    let task_title = ai_result["task_title"].as_str().ok_or_else(|| {
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            "AI response missing task_title",
        )
    })?;
    let task_description = ai_result["task_description"].as_str();
    let project_name = ai_result["project"].as_str();

    let created = state
        .captain
        .add_task_with_context(task_title, project_name, task_description, Some("scout"))
        .await
        .map_err(map_task_create_error)?;

    let task_item: Option<api_types::TaskItem> = state
        .captain
        .load_task(created.id)
        .await
        .ok()
        .flatten()
        .and_then(|t| serde_json::to_value(&t).ok())
        .and_then(|v| serde_json::from_value(v).ok());
    state.bus.send(global_bus::BusPayload::Tasks(Some(
        api_types::TaskEventData {
            action: Some("created".into()),
            item: task_item,
            id: Some(created.id),
            cleared_by: None,
        },
    )));

    Ok(Json(api_types::ActResponse {
        ok: Some(true),
        task_id: Some(created.id.to_string()),
        title: Some(created.title),
        skipped: Some(false),
        reason: None,
    }))
}

// ---------------------------------------------------------------
// PATCH endpoint
// ---------------------------------------------------------------

/// PATCH /api/scout/items/{id}
#[crate::instrument_api(method = "PATCH", path = "/api/scout/items/{id}")]
pub(crate) async fn patch_scout_item(
    State(state): State<AppState>,
    Path(api_types::ScoutItemIdParams { id }): Path<api_types::ScoutItemIdParams>,
    Json(body): Json<api_types::ScoutLifecycleCommandRequest>,
) -> Result<Json<api_types::BoolOkResponse>, ApiError> {
    state
        .scout
        .apply_item_command(id, scout_item_command(body.command))
        .await
        .map_err(|e| {
            map_scout_error(e, |e| {
                internal_error(e, "failed to apply scout lifecycle command")
            })
        })?;
    let scout_payload = match state.scout.get_item_value(id).await {
        Ok(v) => Some(v),
        Err(e) => {
            tracing::warn!(module = "transport-http-transport-routes_scout", scout_id = id, error = %e, "failed to fetch scout item for SSE event");
            None
        }
    };
    let scout_item: Option<api_types::ScoutItem> =
        scout_payload.and_then(|v| serde_json::from_value(v).ok());
    state.bus.send(global_bus::BusPayload::Scout(Some(
        api_types::ScoutEventData {
            action: Some("updated".into()),
            item: scout_item,
            id: Some(id),
        },
    )));
    Ok(Json(api_types::BoolOkResponse { ok: true }))
}

// ---------------------------------------------------------------
// DELETE endpoint
// ---------------------------------------------------------------

/// DELETE /api/scout/items/{id}
#[crate::instrument_api(method = "DELETE", path = "/api/scout/items/{id}")]
pub(crate) async fn delete_scout_item(
    State(state): State<AppState>,
    Path(api_types::ScoutItemIdParams { id }): Path<api_types::ScoutItemIdParams>,
) -> Result<Json<api_types::ScoutDeleteResponse>, ApiError> {
    let val =
        state.scout.delete_item(id).await.map_err(|e| {
            map_scout_error(e, |e| internal_error(e, "failed to delete scout item"))
        })?;
    state.bus.send(global_bus::BusPayload::Scout(Some(
        api_types::ScoutEventData {
            action: Some("deleted".into()),
            item: None,
            id: Some(id),
        },
    )));
    Ok(Json(decode_response(
        val,
        "failed to decode scout delete response",
    )?))
}

/// GET /api/scout/items/{id}/sessions — list CC sessions for a scout item.
#[crate::instrument_api(method = "GET", path = "/api/scout/items/{id}/sessions")]
pub(crate) async fn get_scout_item_sessions(
    State(state): State<AppState>,
    Path(api_types::ScoutItemIdParams { id }): Path<api_types::ScoutItemIdParams>,
) -> Result<Json<Vec<api_types::ScoutItemSession>>, ApiError> {
    let sessions = state
        .scout
        .list_item_sessions(id)
        .await
        .map_err(|e| internal_error(e, "failed to load scout sessions"))?;
    Ok(Json(
        sessions
            .into_iter()
            .map(|session| api_types::ScoutItemSession {
                session_id: session.session_id,
                caller: session.caller,
                status: session.status,
                created_at: session.created_at,
                model: (!session.model.is_empty()).then_some(session.model),
                duration_ms: session.duration_ms,
                cost_usd: session.cost_usd,
            })
            .collect(),
    ))
}
