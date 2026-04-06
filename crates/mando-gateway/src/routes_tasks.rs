//! /api/tasks/* route handlers.

use std::path::PathBuf;

use axum::extract::multipart::Field;
use axum::extract::{Multipart, Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::response::{error_response, internal_error};
use crate::AppState;

/// Extract a text field from a multipart part, returning `Ok(None)` if empty.
async fn field_text(field: Field<'_>) -> Result<Option<String>, (StatusCode, Json<Value>)> {
    let val = field
        .text()
        .await
        .map_err(|e| error_response(StatusCode::BAD_REQUEST, &e.to_string()))?;
    Ok(if val.is_empty() { None } else { Some(val) })
}

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

/// POST /api/tasks/add (multipart: title, project/repo, optional context/plan/no_pr, images)
pub(crate) async fn post_task_add(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<Value>), (StatusCode, Json<Value>)> {
    let mut title = String::new();
    let mut repo: Option<String> = None;
    let mut context: Option<String> = None;
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
                title = field_text(field).await?.unwrap_or_default();
            }
            "project" | "repo" => {
                if let Some(val) = field_text(field).await? {
                    repo = Some(val);
                }
            }
            "context" => {
                if let Some(val) = field_text(field).await? {
                    context = Some(val);
                }
            }
            "plan" => {
                if let Some(val) = field_text(field).await? {
                    plan = Some(val);
                }
            }
            "no_pr" => {
                if let Some(val) = field_text(field).await? {
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

                tokio::fs::create_dir_all(&images_dir).await.map_err(|e| {
                    crate::response::internal_error_with(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        e,
                        "failed to save image",
                    )
                })?;
                tokio::fs::write(images_dir.join(&dest_name), &data)
                    .await
                    .map_err(|e| {
                        crate::response::internal_error_with(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            e,
                            "failed to save image",
                        )
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

        if !saved_images.is_empty() || context.is_some() || plan.is_some() || no_pr.is_some() {
            if let Some(id) = val["id"].as_i64() {
                let mut updates = json!({});
                if !saved_images.is_empty() {
                    updates["images"] = json!(saved_images.join(","));
                }
                if let Some(ref value) = context {
                    updates["context"] = json!(value);
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
                        crate::response::internal_error_with(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            e,
                            "failed to update task",
                        )
                    })?;
            }
        }
        val
    };

    state
        .bus
        .send(mando_types::BusEvent::Tasks, Some(json!({"action": "add"})));

    // Signal the auto-tick loop to run immediately so the new task is
    // dispatched without waiting for the next scheduled interval. The loop
    // watches `mando_captain::WORKER_EXIT_SIGNAL` as its wake trigger.
    if config.captain.auto_schedule {
        mando_captain::WORKER_EXIT_SIGNAL.notify_one();
    }

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

fn images_dir() -> PathBuf {
    mando_config::data_dir().join("images")
}
