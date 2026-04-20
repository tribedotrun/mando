//! /api/tasks/* route handlers.

use axum::extract::multipart::Field;
use axum::extract::{Multipart, Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use captain::EffectRequest;
use captain::UpdateTaskInput;

use crate::response::{
    error_response, internal_error, map_task_create_error, touch_workbench_activity, ApiCreated,
    ApiError,
};
use crate::AppState;

/// Extract a text field from a multipart part, returning `Ok(None)` if empty.
async fn field_text(field: Field<'_>) -> Result<Option<String>, ApiError> {
    let val = field
        .text()
        .await
        .map_err(|e| error_response(StatusCode::BAD_REQUEST, &e.to_string()))?;
    Ok(if val.is_empty() { None } else { Some(val) })
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
#[crate::instrument_api(method = "GET", path = "/api/tasks")]
pub(crate) async fn get_tasks(
    State(state): State<AppState>,
    Query(query): Query<api_types::TaskListQuery>,
) -> Result<Json<api_types::TaskListResponse>, ApiError> {
    let items = state
        .captain
        .load_all_tasks(query.include_archived.unwrap_or(false))
        .await
        .map_err(|e| internal_error(e, "failed to load tasks"))?;
    let items: Vec<api_types::TaskItem> = serde_json::from_value(
        serde_json::to_value(items).map_err(|e| internal_error(e, "failed to serialize tasks"))?,
    )
    .map_err(|e| internal_error(e, "failed to serialize tasks"))?;
    let count = items.len();
    Ok(Json(api_types::TaskListResponse { items, count }))
}

/// POST /api/tasks/add (multipart: title, project/repo, optional context/plan/no_pr, images)
#[crate::instrument_api(method = "POST", path = "/api/tasks/add")]
pub(crate) async fn post_task_add(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<ApiCreated<api_types::TaskItem>, ApiError> {
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

    let config = state.settings.load_config();

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

    let created = {
        let created = state
            .captain
            .add_task(title.trim(), repo.as_deref(), source.as_deref())
            .await
            .map_err(map_task_create_error)?;

        let task_id = created.id;

        // Build a typed patch for the optional fields that came in via multipart.
        let has_updates = !saved_images.is_empty()
            || context.is_some()
            || plan.is_some()
            || no_pr.is_some()
            || no_auto_merge.is_some();

        if has_updates {
            let updates = UpdateTaskInput {
                images: if saved_images.is_empty() {
                    None
                } else {
                    Some(Some(saved_images.join(",")))
                },
                context: context.map(Some),
                plan: plan.as_ref().cloned().map(Some),
                no_pr: no_pr.as_deref().map(|v| v == "true"),
                no_auto_merge: no_auto_merge.as_deref().map(|v| v == "true"),
                ..Default::default()
            };
            state
                .captain
                .update_task(task_id, updates)
                .await
                .map_err(|e| {
                    crate::response::internal_error_with(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        e,
                        "failed to update task",
                    )
                })?;
        }

        if let Some(ref value) = planning {
            state
                .captain
                .set_task_planning(task_id, value == "true")
                .await
                .map_err(|e| {
                    crate::response::internal_error_with(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        e,
                        "failed to update task planning",
                    )
                })?;
        }

        if plan.is_some() || planning.is_some() {
            state
                .captain
                .queue_item(task_id, "task_add")
                .await
                .map_err(|e| {
                    crate::response::internal_error_with(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        e,
                        "failed to queue task",
                    )
                })?;
        }

        created
    };

    // Create a workbench for the new task so it is clickable in the sidebar
    // from the moment it appears.  Uses the same pattern as POST /api/worktrees.
    // Resolve the project from the created task (it may have been inferred).
    let task_id = created.id;
    {
        let project_name = state
            .captain
            .load_task(task_id)
            .await
            .ok()
            .flatten()
            .map(|t| t.project.clone());
        if let Some(ref pname) = project_name {
            if let Err(e) = create_workbench_for_task(&state, task_id, pname, title.trim()).await {
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
    let task_payload = state.captain.task_json(task_id).await.ok().flatten();

    {
        let mut effects = vec![EffectRequest::TaskBusPublish {
            task_id,
            action: "created",
        }];
        if config.captain.auto_schedule {
            effects.push(EffectRequest::WakeupCaptain {
                reason: "task_created",
            });
        }
        state
            .captain
            .enqueue_task_effects(task_id, Some("task_created"), effects)
            .await
            .map_err(|e| internal_error(e, "failed to dispatch task creation side effects"))?;
    }

    let fallback = serde_json::to_value(&created)
        .map_err(|e| internal_error(e, "failed to serialize created task"))?;
    let response = serde_json::from_value(task_payload.unwrap_or(fallback))
        .map_err(|e| internal_error(e, "failed to serialize created task"))?;
    Ok(ApiCreated(response))
}

/// Create a worktree + workbench for a freshly inserted task and link them.
async fn create_workbench_for_task(
    state: &AppState,
    task_id: i64,
    project_name: &str,
    title: &str,
) -> anyhow::Result<()> {
    state
        .captain
        .create_task_workbench(task_id, project_name, title)
        .await
}

/// POST /api/tasks/bulk
#[crate::instrument_api(method = "POST", path = "/api/tasks/bulk")]
pub(crate) async fn post_task_bulk(
    State(state): State<AppState>,
    Json(body): Json<api_types::TaskBulkUpdateRequest>,
) -> Result<Json<api_types::BoolOkResponse>, ApiError> {
    let ids = &body.ids;
    // Map TaskBulkUpdates to UpdateTaskInput. Currently the only supported field is `worker`.
    let updates = UpdateTaskInput {
        worker: body.updates.worker.map(Some),
        ..Default::default()
    };
    match state.captain.bulk_update_tasks(ids, updates).await {
        Ok(()) => {
            state.bus.send(global_bus::BusPayload::Tasks(Some(
                api_types::TaskEventData {
                    action: Some("bulk".into()),
                    item: None,
                    id: None,
                    cleared_by: None,
                },
            )));
            Ok(Json(api_types::BoolOkResponse { ok: true }))
        }
        Err(e) => {
            let msg = e.to_string();
            Err(error_response(task_update_error_status(&e), &msg))
        }
    }
}

/// POST /api/tasks/delete
#[crate::instrument_api(method = "POST", path = "/api/tasks/delete")]
pub(crate) async fn post_task_delete(
    State(state): State<AppState>,
    Json(body): Json<api_types::TaskDeleteRequest>,
) -> Result<Json<api_types::DeleteTasksResponse>, ApiError> {
    let ids = &body.ids;
    match state
        .captain
        .delete_tasks_for_api(
            ids,
            body.close_pr.unwrap_or(true),
            body.force.unwrap_or(false),
        )
        .await
    {
        Ok(warnings) => {
            for id in ids {
                state.bus.send(global_bus::BusPayload::Tasks(Some(
                    api_types::TaskEventData {
                        action: Some("deleted".into()),
                        item: None,
                        id: Some(*id),
                        cleared_by: None,
                    },
                )));
            }
            Ok(Json(api_types::DeleteTasksResponse {
                ok: true,
                deleted: ids.len(),
                warnings: (!warnings.is_empty()).then_some(warnings),
            }))
        }
        Err(e) => Err(internal_error(e, "failed to delete tasks")),
    }
}

/// DELETE /api/tasks  (same logic as POST /api/tasks/delete)
#[crate::instrument_api(method = "DELETE", path = "/api/tasks")]
pub(crate) async fn delete_task_items(
    State(state): State<AppState>,
    Json(body): Json<api_types::TaskDeleteRequest>,
) -> Result<Json<api_types::DeleteTasksResponse>, ApiError> {
    post_task_delete(State(state), Json(body)).await
}

/// POST /api/tasks/merge
#[crate::instrument_api(method = "POST", path = "/api/tasks/merge")]
pub(crate) async fn post_task_merge(
    State(state): State<AppState>,
    Json(body): Json<api_types::MergeRequest>,
) -> Result<Json<api_types::MergeResponse>, ApiError> {
    state
        .captain
        .merge_pr(body.pr_number, &body.project)
        .await
        .map(Json)
        .map_err(|e| internal_error(e, "merge failed"))
}

/// PATCH /api/tasks/{id}
#[crate::instrument_api(method = "PATCH", path = "/api/tasks/{id}")]
pub(crate) async fn patch_task_item(
    State(state): State<AppState>,
    Path(api_types::TaskIdParams { id: id_num }): Path<api_types::TaskIdParams>,
    Json(body): Json<api_types::TaskPatchRequest>,
) -> Result<Json<api_types::BoolOkResponse>, ApiError> {
    // Map TaskPatchRequest fields to UpdateTaskInput.
    let updates = UpdateTaskInput {
        context: body.context.map(Some),
        original_prompt: body.original_prompt.map(Some),
        ..Default::default()
    };
    match state.captain.update_task(id_num, updates).await {
        Ok(()) => {
            let updated = state.captain.task_json(id_num).await.ok().flatten();
            let wb_id = updated
                .as_ref()
                .and_then(|v| v.get("workbench_id"))
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            let task_item: Option<api_types::TaskItem> =
                updated.and_then(|v| serde_json::from_value(v).ok());
            state.bus.send(global_bus::BusPayload::Tasks(Some(
                api_types::TaskEventData {
                    action: Some("updated".into()),
                    item: task_item,
                    id: Some(id_num),
                    cleared_by: None,
                },
            )));
            touch_workbench_activity(&state, wb_id).await;
            Ok(Json(api_types::BoolOkResponse { ok: true }))
        }
        Err(e) => {
            let msg = e.to_string();
            Err(error_response(task_update_error_status(&e), &msg))
        }
    }
}
