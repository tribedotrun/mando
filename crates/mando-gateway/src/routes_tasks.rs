//! /api/tasks/* route handlers.

use std::path::PathBuf;

use axum::extract::{Multipart, Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::response::{error_response, internal_error};
use crate::AppState;

#[derive(Deserialize, Default)]
pub(crate) struct TaskListQuery {
    #[serde(default)]
    pub include_archived: Option<bool>,
}

fn parse_id(s: &str) -> Result<i64, (StatusCode, Json<Value>)> {
    s.parse::<i64>()
        .map_err(|_| error_response(StatusCode::BAD_REQUEST, &format!("invalid id: {s}")))
}

fn task_update_error_status(err: &anyhow::Error) -> StatusCode {
    match err.downcast_ref::<mando_types::TaskUpdateError>() {
        Some(e) if e.is_not_found() => StatusCode::NOT_FOUND,
        Some(e) if e.is_client_error() => StatusCode::BAD_REQUEST,
        Some(_) => StatusCode::INTERNAL_SERVER_ERROR,
        None => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

// ---------------------------------------------------------------
// GET endpoints
// ---------------------------------------------------------------

/// GET /api/tasks
pub(crate) async fn get_tasks(
    State(state): State<AppState>,
    Query(query): Query<TaskListQuery>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let config = state.config.load_full();
    let store = state.task_store.read().await;
    let items = if query.include_archived.unwrap_or(false) {
        store
            .load_all_with_archived()
            .await
            .map_err(internal_error)?
    } else {
        store.load_all().await.map_err(internal_error)?
    };
    let count = items.len();
    let mut items_json: Vec<Value> = items
        .iter()
        .map(serde_json::to_value)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| {
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("serialization failed: {e}"),
            )
        })?;
    // Backfill github_repo from config for tasks created before the column was added
    for (i, item) in items.iter().enumerate() {
        if item.github_repo.is_none() {
            let github_repo = crate::resolve_github_repo(item.project.as_deref(), &config);
            if let Some(obj) = items_json[i].as_object_mut() {
                obj.insert("github_repo".to_string(), json!(github_repo));
            }
        }
    }
    Ok(Json(json!({
        "items": items_json,
        "count": count,
    })))
}

// ---------------------------------------------------------------
// POST endpoints
// ---------------------------------------------------------------

/// POST /api/tasks/add (multipart: title, project/repo, optional context/linear_id/plan/no_pr, images)
pub(crate) async fn post_task_add(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<Value>)> {
    let mut title = String::new();
    let mut repo: Option<String> = None;
    let mut context: Option<String> = None;
    let mut linear_id: Option<String> = None;
    let mut plan: Option<String> = None;
    let mut no_pr: Option<String> = None;
    let mut saved_images: Vec<String> = Vec::new();

    let images_dir = images_dir();

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| error_response(StatusCode::BAD_REQUEST, &format!("multipart error: {e}")))?
    {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "title" => {
                title = field
                    .text()
                    .await
                    .map_err(|e| error_response(StatusCode::BAD_REQUEST, &e.to_string()))?;
            }
            "project" | "repo" => {
                let val = field
                    .text()
                    .await
                    .map_err(|e| error_response(StatusCode::BAD_REQUEST, &e.to_string()))?;
                if !val.is_empty() {
                    repo = Some(val);
                }
            }
            "context" => {
                let val = field
                    .text()
                    .await
                    .map_err(|e| error_response(StatusCode::BAD_REQUEST, &e.to_string()))?;
                if !val.is_empty() {
                    context = Some(val);
                }
            }
            "linear_id" => {
                let val = field
                    .text()
                    .await
                    .map_err(|e| error_response(StatusCode::BAD_REQUEST, &e.to_string()))?;
                if !val.is_empty() {
                    linear_id = Some(val);
                }
            }
            "plan" => {
                let val = field
                    .text()
                    .await
                    .map_err(|e| error_response(StatusCode::BAD_REQUEST, &e.to_string()))?;
                if !val.is_empty() {
                    plan = Some(val);
                }
            }
            "no_pr" => {
                let val = field
                    .text()
                    .await
                    .map_err(|e| error_response(StatusCode::BAD_REQUEST, &e.to_string()))?;
                if !val.is_empty() {
                    no_pr = Some(val);
                }
            }
            "images" => {
                let filename = field.file_name().unwrap_or("upload").to_string();
                let ext = filename
                    .rsplit('.')
                    .next()
                    .filter(|e| e.len() <= 5)
                    .unwrap_or("bin");
                let uuid = mando_uuid::Uuid::v4();
                let dest_name = format!("{uuid}.{ext}");

                let data = field
                    .bytes()
                    .await
                    .map_err(|e| error_response(StatusCode::BAD_REQUEST, &e.to_string()))?;

                std::fs::create_dir_all(&images_dir).map_err(|e| {
                    error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string())
                })?;
                std::fs::write(images_dir.join(&dest_name), &data).map_err(|e| {
                    error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string())
                })?;

                saved_images.push(dest_name);
            }
            _ => {}
        }
    }

    if title.trim().is_empty() {
        return Err(error_response(StatusCode::BAD_REQUEST, "title is required"));
    }

    let config = state.config.load_full();
    let val = {
        let store = state.task_store.read().await;
        let val = mando_captain::runtime::dashboard::add_task(
            &config,
            &store,
            title.trim(),
            repo.as_deref(),
        )
        .await
        .map_err(internal_error)?;

        if !saved_images.is_empty()
            || context.is_some()
            || linear_id.is_some()
            || plan.is_some()
            || no_pr.is_some()
        {
            if let Some(id) = val["id"].as_i64() {
                let mut updates = json!({});
                if !saved_images.is_empty() {
                    updates["images"] = json!(saved_images.join(","));
                }
                if let Some(ref value) = context {
                    updates["context"] = json!(value);
                }
                if let Some(ref value) = linear_id {
                    updates["linear_id"] = json!(value);
                }
                if let Some(ref value) = plan {
                    updates["plan"] = json!(value);
                    updates["status"] = json!("queued");
                }
                if let Some(ref value) = no_pr {
                    updates["no_pr"] = json!(value == "true");
                }
                mando_captain::runtime::dashboard::update_task(&store, id, &updates)
                    .await
                    .map_err(|e| {
                        error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string())
                    })?;
            }
        }
        val
    };

    if linear_id.is_none() {
        if let Some(id) = val["id"].as_i64() {
            create_linear_issue_for_new_item(&state, &config, id).await;
        }
    }

    state
        .bus
        .send(mando_types::BusEvent::Tasks, Some(json!({"action": "add"})));
    Ok((StatusCode::CREATED, Json(val)))
}

#[derive(Deserialize)]
pub(crate) struct BulkBody {
    pub ids: Vec<i64>,
    pub updates: Value,
}

/// POST /api/tasks/bulk
pub(crate) async fn post_task_bulk(
    State(state): State<AppState>,
    Json(body): Json<BulkBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let ids = &body.ids;
    let store = state.task_store.read().await;
    let pool = state.db.pool();
    match mando_captain::runtime::dashboard::bulk_update_tasks(&store, ids, body.updates, pool)
        .await
    {
        Ok(()) => {
            state.bus.send(
                mando_types::BusEvent::Tasks,
                Some(json!({"action": "bulk"})),
            );
            Ok(Json(json!({"ok": true})))
        }
        Err(e) => {
            let msg = e.to_string();
            Err(error_response(task_update_error_status(&e), &msg))
        }
    }
}

#[derive(Deserialize)]
pub(crate) struct DeleteBody {
    pub ids: Vec<i64>,
    #[serde(default)]
    pub close_pr: bool,
    #[serde(default)]
    pub cancel_linear: bool,
}

/// POST /api/tasks/delete
pub(crate) async fn post_task_delete(
    State(state): State<AppState>,
    Json(body): Json<DeleteBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let config = state.config.load_full();
    let ids = &body.ids;
    let opts = mando_captain::io::task_cleanup::CleanupOptions {
        close_pr: body.close_pr,
        cancel_linear: body.cancel_linear,
    };
    let store = state.task_store.read().await;
    match mando_captain::runtime::dashboard::delete_tasks(&config, &store, ids, &opts).await {
        Ok(warnings) => {
            state.bus.send(
                mando_types::BusEvent::Tasks,
                Some(json!({"action": "delete"})),
            );
            let mut resp = json!({"ok": true, "deleted": ids.len()});
            if !warnings.is_empty() {
                resp["warnings"] = json!(warnings);
            }
            Ok(Json(resp))
        }
        Err(e) => Err(error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &e.to_string(),
        )),
    }
}

/// DELETE /api/tasks  (same logic as POST /api/tasks/delete)
pub(crate) async fn delete_task_items(
    State(state): State<AppState>,
    Json(body): Json<DeleteBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    post_task_delete(State(state), Json(body)).await
}

#[derive(Deserialize)]
pub(crate) struct MergeBody {
    pub pr: String,
    pub project: String,
}

/// POST /api/tasks/merge
pub(crate) async fn post_task_merge(
    State(state): State<AppState>,
    Json(body): Json<MergeBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let store = state.task_store.read().await;
    match mando_captain::runtime::dashboard::merge_pr(&store, &body.pr, &body.project).await {
        Ok(val) => Ok(Json(val)),
        Err(e) => Err(error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &e.to_string(),
        )),
    }
}

// ---------------------------------------------------------------
// PATCH endpoint
// ---------------------------------------------------------------

/// PATCH /api/tasks/{id}
pub(crate) async fn patch_task_item(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let id_num = parse_id(&id)?;
    let store = state.task_store.read().await;
    match mando_captain::runtime::dashboard::update_task(&store, id_num, &body).await {
        Ok(()) => {
            state.bus.send(
                mando_types::BusEvent::Tasks,
                Some(json!({"action": "update", "id": id_num})),
            );
            Ok(Json(json!({"ok": true})))
        }
        Err(e) => {
            let msg = e.to_string();
            Err(error_response(task_update_error_status(&e), &msg))
        }
    }
}

// ---------------------------------------------------------------
// Internal
// ---------------------------------------------------------------

pub(crate) async fn create_linear_issue_for_new_item(
    state: &AppState,
    config: &mando_config::settings::Config,
    item_id: i64,
) {
    let item = {
        let store = state.task_store.read().await;
        store.find_by_id(item_id).await.unwrap_or(None)
    };
    let Some(mut item) = item else { return };
    if item.linear_id.is_some() {
        return;
    }
    if let Err(e) =
        mando_captain::runtime::linear_integration::create_issue_for_task(&mut item, config).await
    {
        tracing::warn!(
            module = "tasks",
            item_id = item_id,
            error = %e,
            "failed to create Linear issue for new item"
        );
        return;
    }
    if let Some(ref linear_id) = item.linear_id {
        let store = state.task_store.read().await;
        if let Err(e) = mando_captain::runtime::dashboard::update_task(
            &store,
            item_id,
            &json!({"linear_id": linear_id}),
        )
        .await
        {
            tracing::warn!(
                module = "tasks",
                item_id = item_id,
                linear_id = %linear_id,
                error = %e,
                "created Linear issue but failed to persist linear_id on task"
            );
        }
    }
}

fn images_dir() -> PathBuf {
    mando_config::data_dir().join("images")
}
