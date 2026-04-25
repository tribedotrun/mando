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

/// POST /api/tasks/cancel
#[crate::instrument_api(method = "POST", path = "/api/tasks/cancel")]
pub(crate) async fn post_task_cancel(
    State(state): State<AppState>,
    Json(body): Json<api_types::TaskIdRequest>,
) -> Result<Json<api_types::BoolOkResponse>, ApiError> {
    let id = body.id;
    simple_task_action(&state, id, state.captain.cancel_item(id)).await
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
        .map_err(|e| internal_error(e, "failed to reopen task"))?;

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

    let old_pr_info: Option<(String, String)> = match state.captain.load_task(id).await {
        Ok(Some(item)) => {
            let pr_num = item.pr_number.map(|n| n.to_string());
            let repo = item.github_repo.clone();
            pr_num.zip(repo)
        }
        Err(e) => {
            tracing::warn!(
                module = "gateway",
                task_id = id,
                error = %e,
                "failed to read task for PR close during rework"
            );
            None
        }
        _ => None,
    };

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
