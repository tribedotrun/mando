//! /api/tasks/* route handlers.

use axum::extract::multipart::Field;
use axum::extract::{Multipart, Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use captain::io::git;

use crate::response::{
    error_response, internal_error, map_task_create_error, touch_workbench_activity,
};
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
    match err.downcast_ref::<captain::TaskUpdateError>() {
        Some(e) if e.is_not_found() => StatusCode::NOT_FOUND,
        Some(e) if e.is_client_error() => StatusCode::BAD_REQUEST,
        Some(_) => StatusCode::INTERNAL_SERVER_ERROR,
        None => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

/// GET /api/tasks
pub(crate) async fn get_tasks(
    State(state): State<AppState>,
    Query(query): Query<TaskListQuery>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let store = state.task_store.read().await;
    let items = if query.include_archived.unwrap_or(false) {
        store
            .load_all_with_archived()
            .await
            .map_err(|e| internal_error(e, "failed to load tasks"))?
    } else {
        store
            .load_all()
            .await
            .map_err(|e| internal_error(e, "failed to load tasks"))?
    };
    let count = items.len();
    let items_json: Vec<Value> = items
        .iter()
        .map(serde_json::to_value)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| internal_error(e, "failed to serialize tasks"))?;
    Ok(Json(json!({
        "items": items_json,
        "count": count,
    })))
}

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
    let mut no_auto_merge: Option<String> = None;
    let mut planning: Option<String> = None;
    let mut source: Option<String> = None;
    let mut saved_images: Vec<String> = Vec::new();

    let images_dir = global_infra::paths::images_dir();

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
            "context" => context = field_text(field).await?.or(context),
            "plan" => plan = field_text(field).await?.or(plan),
            "no_pr" => no_pr = field_text(field).await?.or(no_pr),
            "no_auto_merge" => no_auto_merge = field_text(field).await?.or(no_auto_merge),
            "planning" => planning = field_text(field).await?.or(planning),
            "source" => source = field_text(field).await?.or(source),
            "images" => {
                let filename = field.file_name().unwrap_or("upload").to_string();
                let ext = filename
                    .rsplit('.')
                    .next()
                    .filter(|e| e.len() <= 5)
                    .unwrap_or("bin");
                let uuid = global_infra::uuid::Uuid::v4();
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

    // Validate project name before calling add_task so the client gets a 400
    // with the helpful message instead of a generic 500.
    if let Some(ref name) = repo {
        if settings::config::resolve_project_config(Some(name), &config).is_none() {
            let mut valid: Vec<&str> = config
                .captain
                .projects
                .values()
                .map(|pc| pc.name.as_str())
                .collect();
            valid.sort_unstable();
            let list = if valid.is_empty() {
                "(none configured)".to_string()
            } else {
                valid.join(", ")
            };
            return Err(error_response(
                StatusCode::BAD_REQUEST,
                &format!("unknown project {name:?} — valid projects: {list}"),
            ));
        }
    }

    let val = {
        let store = state.task_store.read().await;
        let val = captain::runtime::dashboard::add_task(
            &config,
            &store,
            title.trim(),
            repo.as_deref(),
            source.as_deref(),
        )
        .await
        .map_err(map_task_create_error)?;

        if !saved_images.is_empty()
            || context.is_some()
            || plan.is_some()
            || no_pr.is_some()
            || no_auto_merge.is_some()
            || planning.is_some()
        {
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
                if let Some(ref value) = no_auto_merge {
                    updates["no_auto_merge"] = json!(value == "true");
                }
                if let Some(ref value) = planning {
                    updates["planning"] = json!(value == "true");
                    updates["status"] = json!("queued");
                }
                captain::runtime::dashboard::update_task(&store, id, &updates)
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

    // Create a workbench for the new task so it is clickable in the sidebar
    // from the moment it appears.  Uses the same pattern as POST /api/worktrees.
    // Resolve the project from the created task (it may have been inferred).
    if let Some(task_id) = val["id"].as_i64() {
        let project_name = {
            let store = state.task_store.read().await;
            store
                .find_by_id(task_id)
                .await
                .ok()
                .flatten()
                .map(|t| t.project.clone())
        };
        if let Some(ref pname) = project_name {
            if let Err(e) =
                create_workbench_for_task(&state, &config, task_id, pname, title.trim()).await
            {
                // Non-fatal: the task exists, captain will create the workbench
                // at spawn time if we fail here.
                tracing::warn!(
                    module = "tasks",
                    task_id,
                    error = %e,
                    "failed to create workbench at task creation"
                );
            }
        }
    }

    // Reload the created task to include any field updates (context, plan, images, workbench_id).
    let task_payload = if let Some(id) = val["id"].as_i64() {
        let store = state.task_store.read().await;
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
        global_types::BusEvent::Tasks,
        Some(json!({"action": "created", "item": task_payload, "id": val["id"]})),
    );

    // Signal the auto-tick loop to run immediately so the new task is
    // dispatched without waiting for the next scheduled interval. The loop
    // watches `captain::WORKER_EXIT_SIGNAL` as its wake trigger.
    if config.captain.auto_schedule {
        captain::WORKER_EXIT_SIGNAL.notify_one();
    }

    let response = task_payload.unwrap_or(val);
    Ok((StatusCode::CREATED, Json(response)))
}

/// Create a worktree + workbench for a freshly inserted task and link them.
async fn create_workbench_for_task(
    state: &AppState,
    _config: &settings::config::Config,
    task_id: i64,
    project_name: &str,
    title: &str,
) -> anyhow::Result<()> {
    let pool = state.db.pool();

    // Resolve project from DB.
    let project_row = settings::io::projects::resolve(pool, project_name)
        .await?
        .ok_or_else(|| anyhow::anyhow!("project not found: {project_name}"))?;
    let project_path = global_infra::paths::expand_tilde(&project_row.path);

    // Build worktree path: {worktrees_dir}/{repo}-todo-{task_id}
    let suffix = format!("todo-{task_id}");
    let branch = format!("mando/{suffix}");
    let wt_path = git::worktree_path(&project_path, &suffix);

    // Create the git worktree.
    git::fetch_origin(&project_path).await?;
    let default_br = git::default_branch(&project_path).await?;
    if wt_path.exists() {
        let _ = git::remove_worktree(&project_path, &wt_path).await;
    }
    let _ = git::delete_local_branch(&project_path, &branch).await;
    git::create_worktree(&project_path, &branch, &wt_path, &default_br).await?;

    // Insert workbench row.
    let wb = captain::Workbench::new(
        project_row.id,
        project_row.name.clone(),
        wt_path.to_string_lossy().to_string(),
        title.to_string(),
    );
    let wb_id = captain::io::queries::workbenches::insert(pool, &wb).await?;

    // Link the workbench to the task.
    let store = state.task_store.read().await;
    captain::runtime::dashboard::update_task(&store, task_id, &json!({"workbench_id": wb_id}))
        .await?;

    state.bus.send(global_types::BusEvent::Workbenches, None);

    Ok(())
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
    match captain::runtime::dashboard::bulk_update_tasks(&store, ids, body.updates, pool).await {
        Ok(()) => {
            state.bus.send(
                global_types::BusEvent::Tasks,
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
    pub force: bool,
}

/// POST /api/tasks/delete
pub(crate) async fn post_task_delete(
    State(state): State<AppState>,
    Json(body): Json<DeleteBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let config = state.config.load_full();
    let ids = &body.ids;
    let opts = captain::io::task_cleanup::CleanupOptions {
        close_pr: true,
        force: body.force,
    };
    let store = state.task_store.read().await;
    match captain::runtime::dashboard::delete_tasks(&config, &store, ids, &opts).await {
        Ok(warnings) => {
            for id in ids {
                state.bus.send(
                    global_types::BusEvent::Tasks,
                    Some(json!({"action": "deleted", "id": id})),
                );
            }
            let mut resp = json!({"ok": true, "deleted": ids.len()});
            if !warnings.is_empty() {
                resp["warnings"] = json!(warnings);
            }
            Ok(Json(resp))
        }
        Err(e) => Err(internal_error(e, "failed to delete tasks")),
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
    pub pr_number: i64,
    pub project: String,
}

/// POST /api/tasks/merge
pub(crate) async fn post_task_merge(
    State(state): State<AppState>,
    Json(body): Json<MergeBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let store = state.task_store.read().await;
    match captain::runtime::dashboard::merge_pr(&store, body.pr_number, &body.project).await {
        Ok(val) => Ok(Json(val)),
        Err(e) => Err(internal_error(e, "merge failed")),
    }
}

/// PATCH /api/tasks/{id}
pub(crate) async fn patch_task_item(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let id_num = parse_id(&id)?;
    let store = state.task_store.read().await;
    match captain::runtime::dashboard::update_task(&store, id_num, &body).await {
        Ok(()) => {
            let updated = store
                .find_by_id(id_num)
                .await
                .ok()
                .flatten()
                .map(|t| serde_json::to_value(&t).unwrap());
            let wb_id = updated
                .as_ref()
                .and_then(|v| v.get("workbench_id"))
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            state.bus.send(
                global_types::BusEvent::Tasks,
                Some(json!({"action": "updated", "item": updated, "id": id_num})),
            );
            drop(store);
            touch_workbench_activity(&state, wb_id).await;
            Ok(Json(json!({"ok": true})))
        }
        Err(e) => {
            let msg = e.to_string();
            Err(error_response(task_update_error_status(&e), &msg))
        }
    }
}
