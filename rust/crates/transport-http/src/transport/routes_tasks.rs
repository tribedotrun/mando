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

#[derive(Default)]
struct TaskAddMultipartFields {
    title: String,
    repo: Option<String>,
    context: Option<String>,
    plan: Option<String>,
    no_pr: Option<String>,
    no_auto_merge: Option<String>,
    planning: Option<String>,
    source: Option<String>,
    saved_images: Vec<String>,
}

async fn extract_task_add_multipart(
    mut multipart: Multipart,
) -> Result<TaskAddMultipartFields, ApiError> {
    let mut fields = TaskAddMultipartFields::default();

    let result = async {
        while let Some(field) = multipart.next_field().await.map_err(|e| {
            error_response(StatusCode::BAD_REQUEST, &format!("multipart error: {e}"))
        })? {
            let name = field.name().unwrap_or("").to_string();
            match name.as_str() {
                "title" => {
                    fields.title = field_text(field).await?.unwrap_or_default();
                }
                "project" | "repo" => {
                    if let Some(val) = field_text(field).await? {
                        fields.repo = Some(val);
                    }
                }
                "context" => fields.context = field_text(field).await?.or(fields.context.take()),
                "plan" => fields.plan = field_text(field).await?.or(fields.plan.take()),
                "no_pr" => fields.no_pr = field_text(field).await?.or(fields.no_pr.take()),
                "no_auto_merge" => {
                    fields.no_auto_merge = field_text(field).await?.or(fields.no_auto_merge.take())
                }
                "planning" => fields.planning = field_text(field).await?.or(fields.planning.take()),
                "source" => fields.source = field_text(field).await?.or(fields.source.take()),
                "images" => {
                    fields
                        .saved_images
                        .push(crate::image_upload::save_image_field(field).await?);
                }
                _ => {
                    return Err(crate::image_upload::unexpected_multipart_field(Some(
                        name.as_str(),
                    )))
                }
            }
        }
        Ok(())
    }
    .await;

    if let Err(err) = result {
        crate::image_upload::cleanup_saved_images(&fields.saved_images).await;
        return Err(err);
    }

    Ok(fields)
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
    multipart: Multipart,
) -> Result<ApiCreated<api_types::TaskItem>, ApiError> {
    let TaskAddMultipartFields {
        title,
        repo,
        context,
        plan,
        no_pr,
        no_auto_merge,
        planning,
        source,
        saved_images,
    } = extract_task_add_multipart(multipart).await?;

    if title.trim().is_empty() {
        crate::image_upload::cleanup_saved_images(&saved_images).await;
        return Err(error_response(StatusCode::BAD_REQUEST, "title is required"));
    }

    let config = state.settings.load_config();

    // Validate project name before calling add_task so the client gets a 400
    // with the helpful message instead of a generic 500.
    if let Some(ref name) = repo {
        if settings::resolve_project_config(Some(name), &config).is_none() {
            crate::image_upload::cleanup_saved_images(&saved_images).await;
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
        let created = match state
            .captain
            .add_task(title.trim(), repo.as_deref(), source.as_deref())
            .await
            .map_err(map_task_create_error)
        {
            Ok(created) => created,
            Err(err) => {
                crate::image_upload::cleanup_saved_images(&saved_images).await;
                return Err(err);
            }
        };

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
            if let Err(err) = state.captain.update_task(task_id, updates).await {
                crate::image_upload::cleanup_saved_images(&saved_images).await;
                return Err(crate::response::internal_error_with(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    err,
                    "failed to update task",
                ));
            }
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

    let response = match task_payload {
        Some(task) => task,
        None => serde_json::from_value(
            serde_json::to_value(&created)
                .map_err(|e| internal_error(e, "failed to serialize created task"))?,
        )
        .map_err(|e| internal_error(e, "failed to convert created task to api type"))?,
    };
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
            emit_task_delete_events(&state.bus, ids);
            Ok(Json(api_types::DeleteTasksResponse {
                ok: true,
                deleted: ids.len(),
                warnings: (!warnings.is_empty()).then_some(warnings),
            }))
        }
        Err(e) => Err(internal_error(e, "failed to delete tasks")),
    }
}

/// Emit bus events after a successful task delete: one `Tasks(deleted)` event
/// per id, then a single `Workbenches(None)` resync so renderers drop the
/// soft-deleted workbench rows that `cleanup_task` wrote.
fn emit_task_delete_events(bus: &global_bus::EventBus, ids: &[i64]) {
    for id in ids {
        bus.send(global_bus::BusPayload::Tasks(Some(
            api_types::TaskEventData {
                action: Some("deleted".into()),
                item: None,
                id: Some(*id),
                cleared_by: None,
            },
        )));
    }
    bus.send(global_bus::BusPayload::Workbenches(None));
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
            let wb_id = updated.as_ref().map(|task| task.workbench_id).unwrap_or(0);
            let task_item = updated;
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

#[cfg(test)]
mod tests {
    use super::*;
    use global_bus::{BusPayload, EventBus};

    #[tokio::test]
    async fn emit_task_delete_events_sends_tasks_and_workbenches_resync() {
        let bus = EventBus::new();
        let mut rx = bus.subscribe();

        emit_task_delete_events(&bus, &[42, 43]);

        let mut deleted_ids = Vec::new();
        let mut workbench_resync = false;
        for _ in 0..3 {
            match rx.recv().await.expect("bus recv") {
                BusPayload::Tasks(Some(data)) => {
                    assert_eq!(data.action.as_deref(), Some("deleted"));
                    if let Some(id) = data.id {
                        deleted_ids.push(id);
                    }
                }
                BusPayload::Workbenches(None) => workbench_resync = true,
                other => panic!("unexpected bus payload: {other:?}"),
            }
        }
        deleted_ids.sort_unstable();
        assert_eq!(deleted_ids, vec![42, 43]);
        assert!(
            workbench_resync,
            "expected Workbenches(None) resync event after task delete"
        );
    }
}
