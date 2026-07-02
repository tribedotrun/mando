//! Task lifecycle-action route handlers (accept, cancel, reopen, rework, handoff).

use std::future::Future;

use api_types::TimelineEventPayload;
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use captain::find_task_action_error;
use captain::EffectRequest;

use crate::response::{error_response, internal_error, touch_workbench_activity, ApiError};
use crate::AppState;

fn map_task_action_error(err: anyhow::Error, context: &'static str) -> ApiError {
    if let Some(typed) = find_task_action_error(&err) {
        let message = typed.to_string();
        let status = if typed.is_not_found() {
            StatusCode::NOT_FOUND
        } else if typed.is_conflict() {
            StatusCode::CONFLICT
        } else {
            StatusCode::INTERNAL_SERVER_ERROR
        };
        return error_response(status, &message);
    }
    internal_error(err, context)
}

/// Shared wrapper for simple task actions that return `anyhow::Result<()>`,
/// then emit a Tasks bus event on success.
async fn simple_task_action<Fut>(
    _state: &AppState,
    _id: i64,
    work: Fut,
) -> Result<Json<api_types::BoolOkResponse>, ApiError>
where
    Fut: Future<Output = anyhow::Result<()>>,
{
    work.await
        .map_err(|e| map_task_action_error(e, "task action failed"))?;
    Ok(Json(api_types::BoolOkResponse { ok: true }))
}

fn build_implementation_context(
    existing_context: Option<&str>,
    timeline: &[captain::TimelineEvent],
    message: &str,
) -> String {
    let plan = timeline.iter().rev().find_map(|event| match &event.data {
        TimelineEventPayload::PlanCompleted { plan, .. } => Some(plan.trim()),
        _ => None,
    });
    let mut sections: Vec<String> = Vec::new();
    if let Some(existing) = existing_context
        .map(str::trim_end)
        .filter(|text| !text.is_empty())
    {
        sections.push(existing.to_string());
    }
    if let Some(plan) = plan.filter(|text| !text.is_empty()) {
        sections.push(format!("## Approved Plan\n{plan}"));
    }
    sections.push(format!("[Human] {message}"));
    sections.join("\n\n")
}

/// POST /api/tasks/implement
#[crate::instrument_api(method = "POST", path = "/api/tasks/implement")]
pub(crate) async fn post_task_implement(
    State(state): State<AppState>,
    Json(body): Json<api_types::TaskImplementRequest>,
) -> Result<Json<api_types::BoolOkResponse>, ApiError> {
    let id = body.id;
    let item = state
        .captain
        .load_task(id)
        .await
        .map_err(|e| internal_error(e, "failed to load task"))?
        .ok_or_else(|| {
            map_task_action_error(
                captain::TaskActionError::NotFound(id).into(),
                "failed to load task",
            )
        })?;
    if item.status() != captain::ItemStatus::PlanReady {
        return Err(map_task_action_error(
            captain::TaskActionError::InvalidTransition {
                command: "start implementation",
                status: item.status().as_str(),
            }
            .into(),
            "failed to start implementation",
        ));
    }

    let timeline = state
        .captain
        .task_timeline(&id.to_string())
        .await
        .map_err(|e| internal_error(e, "failed to load task timeline"))?;
    let context = build_implementation_context(item.context.as_deref(), &timeline, &body.message);
    state
        .captain
        .update_task(
            id,
            captain::UpdateTaskInput {
                context: Some(Some(context)),
                ..Default::default()
            },
        )
        .await
        .map_err(|e| internal_error(e, "failed to update task context"))?;
    simple_task_action(
        &state,
        id,
        state.captain.queue_item(id, "http_start_implementation"),
    )
    .await
}

/// POST /api/tasks/queue
#[crate::instrument_api(method = "POST", path = "/api/tasks/queue")]
pub(crate) async fn post_task_queue(
    State(state): State<AppState>,
    Json(body): Json<api_types::TaskIdRequest>,
) -> Result<Json<api_types::BoolOkResponse>, ApiError> {
    let id = body.id;
    simple_task_action(&state, id, state.captain.queue_item(id, "http_queue")).await
}

/// POST /api/tasks/accept
#[crate::instrument_api(method = "POST", path = "/api/tasks/accept")]
pub(crate) async fn post_task_accept(
    State(state): State<AppState>,
    Json(body): Json<api_types::TaskIdRequest>,
) -> Result<Json<api_types::BoolOkResponse>, ApiError> {
    let id = body.id;
    simple_task_action(&state, id, state.captain.accept_item(id)).await
}

/// Extract `(pr_number, github_repo)` from a task for best-effort PR close.
/// Returns `Some` only when BOTH a pr_number and github_repo are present.
fn task_pr_close_info(item: &captain::Task) -> Option<(String, String)> {
    item.pr_number
        .map(|n| n.to_string())
        .zip(item.github_repo.clone())
}

/// Load a task and extract its PR-close info. Best-effort: warns and returns
/// `None` when the task can't be read or has no PR/repo.
async fn load_task_pr_close_info(state: &AppState, id: i64) -> Option<(String, String)> {
    match state.captain.load_task(id).await {
        Ok(Some(item)) => task_pr_close_info(&item),
        Err(e) => {
            tracing::warn!(
                module = "gateway",
                task_id = id,
                error = %e,
                "failed to read task for PR close"
            );
            None
        }
        _ => None,
    }
}

/// POST /api/tasks/cancel
#[crate::instrument_api(method = "POST", path = "/api/tasks/cancel")]
pub(crate) async fn post_task_cancel(
    State(state): State<AppState>,
    Json(body): Json<api_types::TaskIdRequest>,
) -> Result<Json<api_types::BoolOkResponse>, ApiError> {
    let id = body.id;
    let old_pr_info = load_task_pr_close_info(&state, id).await;
    state
        .captain
        .cancel_item(id)
        .await
        .map_err(|e| map_task_action_error(e, "failed to cancel task"))?;
    if let Some((pr_num, repo)) = old_pr_info {
        if let Err(e) = state.captain.close_pr(&repo, &pr_num).await {
            tracing::warn!(
                module = "gateway",
                task_id = id,
                pr = %pr_num,
                error = %e,
                "failed to close PR during cancel — continuing anyway"
            );
        }
    }
    Ok(Json(api_types::BoolOkResponse { ok: true }))
}

/// POST /api/tasks/reopen (JSON or multipart with optional images)
#[crate::instrument_api(method = "POST", path = "/api/tasks/reopen")]
pub(crate) async fn post_task_reopen(
    State(state): State<AppState>,
    request: axum::extract::Request,
) -> Result<Json<api_types::BoolOkResponse>, ApiError> {
    let body = crate::image_upload::extract_feedback(request).await?;
    let result = post_task_reopen_inner(&state, &body).await;
    if result.is_err() {
        crate::image_upload::cleanup_saved_images(&body.saved_images).await;
    }
    result
}

async fn post_task_reopen_inner(
    state: &AppState,
    body: &crate::image_upload::FeedbackWithImages,
) -> Result<Json<api_types::BoolOkResponse>, ApiError> {
    let id = body.id;
    let workflow = state.settings.load_captain_workflow();
    let config = state.settings.load_config();
    let notifier = crate::captain_notifier(state, &config);
    let mut item = state
        .captain
        .load_task(id)
        .await
        .map_err(|e| internal_error(e, "failed to load task"))?
        .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "item not found"))?;

    if !body.saved_images.is_empty() {
        let joined = body.saved_images.join(",");
        item.images = Some(match item.images.take() {
            Some(existing) if !existing.is_empty() => format!("{existing},{joined}"),
            _ => joined,
        });
    }

    let previous_status = item.status();

    crate::runtime::task_sessions::close_ask_session(state, id).await;

    let old_session_id = item.session_ids.worker.clone();
    let outcome = state
        .captain
        .reopen_item_from_human(&mut item, &body.feedback, &workflow, &notifier)
        .await
        .map_err(|e| map_task_action_error(e, "failed to reopen task"))?;

    let summary = match outcome {
        captain::ReopenOutcome::QueuedFallback => {
            if body.feedback.is_empty() {
                "Reopened — queued for fresh work".to_string()
            } else {
                format!("Reopened — queued for fresh work: {}", body.feedback)
            }
        }
        captain::ReopenOutcome::CaptainReviewing => {
            if body.feedback.is_empty() {
                "Reopen routed to captain review".to_string()
            } else {
                format!("Reopen routed to captain review: {}", body.feedback)
            }
        }
        _ => {
            if body.feedback.is_empty() {
                "Reopened".to_string()
            } else {
                format!("Reopened: {}", body.feedback)
            }
        }
    };
    let event = captain::TimelineEvent {
        timestamp: global_types::now_rfc3339(),
        actor: "human".to_string(),
        summary,
        data: TimelineEventPayload::HumanReopen {
            content: body.feedback.clone(),
            worker: item.worker.clone().unwrap_or_default(),
            session_id: item.session_ids.worker.clone().unwrap_or_default(),
            from: previous_status.into(),
            to: item.status().into(),
            source: "direct".to_string(),
        },
    };
    let mut effects: Vec<EffectRequest> = Vec::new();
    effects.push(EffectRequest::TaskBusPublish {
        task_id: item.id,
        action: "updated",
    });
    effects.push(EffectRequest::WorkbenchTouch {
        workbench_id: item.workbench_id,
    });

    if matches!(outcome, captain::ReopenOutcome::Reopened) {
        let truly_resumed = old_session_id.is_some() && old_session_id == item.session_ids.worker;
        let (evt_payload, summary) = if truly_resumed {
            (
                TimelineEventPayload::SessionResumed {
                    worker: item.worker.clone().unwrap_or_default(),
                    session_id: item.session_ids.worker.clone().unwrap_or_default(),
                },
                format!("Resumed {}", item.worker.as_deref().unwrap_or("worker")),
            )
        } else {
            (
                TimelineEventPayload::WorkerSpawned {
                    worker: item.worker.clone().unwrap_or_default(),
                    session_id: item.session_ids.worker.clone().unwrap_or_default(),
                },
                format!("Spawned {}", item.worker.as_deref().unwrap_or("worker")),
            )
        };
        let _ignored = state
            .captain
            .emit_task_timeline_event(&item, &summary, evt_payload)
            .await;

        let msg = if body.feedback.is_empty() {
            format!(
                "\u{1f504} Reopened <b>{}</b>",
                global_infra::html::escape_html(&item.title)
            )
        } else {
            format!(
                "\u{1f504} Reopened <b>{}</b>: {}",
                global_infra::html::escape_html(&item.title),
                global_infra::html::escape_html(&body.feedback)
            )
        };
        effects.push(EffectRequest::NotifyNormal { message: msg });
    }

    if matches!(outcome, captain::ReopenOutcome::CaptainReviewing) {
        state
            .captain
            .enqueue_task_effects(item.id, Some("human_reopen_review"), effects)
            .await
            .map_err(|e| internal_error(e, "failed to publish reopen side effects"))?;
        crate::runtime::task_sessions::clear_advisor_session(state, id).await;
        return Ok(Json(api_types::BoolOkResponse { ok: true }));
    }

    let applied = state
        .captain
        .persist_task_transition_with_effects(&item, previous_status.as_str(), &event, effects)
        .await
        .map_err(|e| internal_error(e, "failed to save reopen transition"))?;
    if !applied {
        return Err(error_response(
            StatusCode::CONFLICT,
            "task changed concurrently while reopening",
        ));
    }

    crate::runtime::task_sessions::clear_advisor_session(state, id).await;
    Ok(Json(api_types::BoolOkResponse { ok: true }))
}

/// POST /api/tasks/rework (JSON or multipart with optional images)
#[crate::instrument_api(method = "POST", path = "/api/tasks/rework")]
pub(crate) async fn post_task_rework(
    State(state): State<AppState>,
    request: axum::extract::Request,
) -> Result<Json<api_types::BoolOkResponse>, ApiError> {
    let body = crate::image_upload::extract_feedback(request).await?;
    let result = post_task_rework_inner(&state, &body).await;
    if result.is_err() {
        crate::image_upload::cleanup_saved_images(&body.saved_images).await;
    }
    result
}

async fn post_task_rework_inner(
    state: &AppState,
    body: &crate::image_upload::FeedbackWithImages,
) -> Result<Json<api_types::BoolOkResponse>, ApiError> {
    let id = body.id;

    crate::runtime::task_sessions::close_ask_session(state, id).await;

    let old_pr_info: Option<(String, String)> = load_task_pr_close_info(state, id).await;

    state
        .captain
        .rework_item(id, &body.feedback)
        .await
        .map_err(|e| map_task_action_error(e, "failed to rework task"))?;
    crate::runtime::task_sessions::clear_advisor_session(state, id).await;

    if !body.saved_images.is_empty() {
        if let Err(e) = state
            .captain
            .append_task_images(id, &body.saved_images)
            .await
        {
            tracing::warn!(module = "transport-http-transport-routes_task_actions", task_id = id, error = ?e, "failed to persist rework images");
        }
    }

    if let Some((pr_num, repo)) = old_pr_info {
        if let Err(e) = state.captain.close_pr(&repo, &pr_num).await {
            tracing::warn!(
                module = "gateway",
                task_id = id,
                pr = %pr_num,
                error = %e,
                "failed to close old PR during rework — continuing anyway"
            );
        }
    }

    Ok(Json(api_types::BoolOkResponse { ok: true }))
}

/// POST /api/tasks/retry — re-trigger CaptainReviewing for Errored items.
#[crate::instrument_api(method = "POST", path = "/api/tasks/retry")]
pub(crate) async fn post_task_retry(
    State(state): State<AppState>,
    Json(body): Json<api_types::TaskIdRequest>,
) -> Result<Json<api_types::BoolOkResponse>, ApiError> {
    let id = body.id;
    state
        .captain
        .retry_item(id)
        .await
        .map_err(|e| map_task_action_error(e, "failed to retry task"))?;
    Ok(Json(api_types::BoolOkResponse { ok: true }))
}

/// POST /api/tasks/resume-rate-limited — clear global rate-limit cooldown and
/// trigger a captain tick so that the identified task (and any others blocked
/// by the cooldown) are picked up immediately.
#[crate::instrument_api(method = "POST", path = "/api/tasks/resume-rate-limited")]
pub(crate) async fn post_task_resume_rate_limited(
    State(state): State<AppState>,
    Json(body): Json<api_types::TaskIdRequest>,
) -> Result<Json<api_types::BoolOkResponse>, ApiError> {
    let id = body.id;
    state
        .captain
        .validate_rate_limited_task(id)
        .await
        .map_err(|e| internal_error(e, "failed to validate rate-limited task"))?;
    if let Some(item) = state
        .captain
        .load_task(id)
        .await
        .map_err(|e| internal_error(e, "failed to load task"))?
    {
        let _ignored = state
            .captain
            .emit_task_timeline_event(
                &item,
                "Rate-limit cooldown cleared manually — resuming",
                TimelineEventPayload::RateLimitCleared {
                    action: "resume-rate-limited".to_string(),
                    cleared_by: "human".to_string(),
                },
            )
            .await;
    }
    let updated = state.captain.task_json(id).await.ok().flatten();
    let wb_id = updated.as_ref().map(|task| task.workbench_id).unwrap_or(0);
    let task_item = updated;
    state.bus.send(global_bus::BusPayload::Tasks(Some(
        api_types::TaskEventData {
            action: Some("updated".into()),
            item: task_item,
            id: Some(id),
            cleared_by: None,
        },
    )));
    touch_workbench_activity(&state, wb_id).await;

    let workflow = state.settings.load_captain_workflow();
    state
        .captain
        .trigger_captain_tick(&workflow, false, false)
        .await
        .map_err(|e| internal_error(e, "failed to trigger captain tick"))?;
    Ok(Json(api_types::BoolOkResponse { ok: true }))
}

/// POST /api/tasks/handoff
#[crate::instrument_api(method = "POST", path = "/api/tasks/handoff")]
pub(crate) async fn post_task_handoff(
    State(state): State<AppState>,
    Json(body): Json<api_types::TaskIdRequest>,
) -> Result<Json<api_types::BoolOkResponse>, ApiError> {
    let id = body.id;
    simple_task_action(&state, id, state.captain.handoff_item(id)).await
}

/// POST /api/tasks/stop — per-task stop. Kills the worker for this task only,
/// transitions status to `stopped`, preserves the worktree for inspection.
/// Reopen resumes the existing session in the existing worktree.
#[crate::instrument_api(method = "POST", path = "/api/tasks/stop")]
pub(crate) async fn post_task_stop(
    State(state): State<AppState>,
    Json(body): Json<api_types::TaskIdRequest>,
) -> Result<Json<api_types::BoolOkResponse>, ApiError> {
    let id = body.id;
    simple_task_action(&state, id, state.captain.stop_item(id)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task_with(pr: Option<i64>, repo: Option<&str>) -> captain::Task {
        let mut item = captain::Task::new("test");
        item.pr_number = pr;
        item.github_repo = repo.map(str::to_string);
        item
    }

    #[test]
    fn close_info_present_when_pr_and_repo_set() {
        let info = task_pr_close_info(&task_with(Some(42), Some("owner/repo")));
        assert_eq!(info, Some(("42".to_string(), "owner/repo".to_string())));
    }

    #[test]
    fn close_info_none_without_pr() {
        assert_eq!(
            task_pr_close_info(&task_with(None, Some("owner/repo"))),
            None
        );
    }

    #[test]
    fn close_info_none_without_repo() {
        assert_eq!(task_pr_close_info(&task_with(Some(7), None)), None);
    }

    #[test]
    fn close_info_none_when_both_missing() {
        assert_eq!(task_pr_close_info(&task_with(None, None)), None);
    }
}
