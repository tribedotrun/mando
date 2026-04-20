//! Task advisor route handlers -- persistent per-task advisor sessions.

use api_types::TimelineEventPayload;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use captain::EffectRequest;

use super::routes_task_advisor_helpers::{action_eligible, build_advisor_prompt};
use crate::response::{
    broadcast_task_update, error_response, internal_error, resolve_task_cwd,
    touch_workbench_activity, ApiError,
};
use crate::AppState;

const PENDING_SESSION: &str = "pending";

/// POST /api/tasks/{id}/advisor -- send a message to the task's advisor.
///
/// Lazy-spawns a CC session on the first message. Follow-up messages
/// resume the same session via `--resume`.
#[crate::instrument_api(method = "POST", path = "/api/tasks/{id}/advisor")]
pub(crate) async fn post_task_advisor(
    State(state): State<AppState>,
    Path(api_types::TaskIdParams { id: task_id }): Path<api_types::TaskIdParams>,
    Json(body): Json<api_types::AdvisorRequest>,
) -> Result<Json<api_types::AdvisorResponse>, ApiError> {
    if body.message.trim().is_empty() {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "message must not be empty",
        ));
    }

    let workflow = state.settings.load_captain_workflow();

    let item = state
        .captain
        .load_task(task_id)
        .await
        .map_err(|e| internal_error(e, "failed to load task"))?
        .ok_or_else(|| {
            error_response(StatusCode::NOT_FOUND, &format!("item {task_id} not found"))
        })?;

    let cwd = resolve_task_cwd(&item, &state)?;
    let session_key = format!("advisor:{task_id}");
    let sessions = state.sessions.clone();

    let mgr_has_session = sessions.has_session(&session_key);
    let task_has_session = item.session_ids.advisor.is_some();
    let should_resume = mgr_has_session && task_has_session;

    if mgr_has_session && !task_has_session {
        tracing::info!(
            module = "transport-http-transport-routes_task_advisor",
            task_id,
            "session_ids.advisor cleared -- closing stale session"
        );
        sessions.close(&session_key);
    } else if !mgr_has_session && task_has_session {
        tracing::warn!(
            module = "transport-http-transport-routes_task_advisor",
            task_id,
            "stale session_ids.advisor -- clearing"
        );
        if let Err(e) = state.captain.set_task_advisor_session(task_id, None).await {
            tracing::warn!(module = "transport-http-transport-routes_task_advisor", task_id, error = %e, "failed to clear session_ids.advisor");
        }
    }

    let ask_id = global_infra::uuid::Uuid::v4().to_string();

    state
        .captain
        .persist_task_question(task_id, &ask_id, PENDING_SESSION, &body.message)
        .await
        .map_err(|e| internal_error(e, "failed to persist advisor question"))?;
    broadcast_task_update(&state, task_id).await;

    let result = run_advisor_cc(
        &state,
        &sessions,
        &session_key,
        should_resume,
        &body.message,
        &item,
        &workflow,
        task_id,
        &ask_id,
        &cwd,
    )
    .await?;

    let answer = result.text.clone();
    let session_id = result.session_id.clone();

    if let Err(e) = state
        .captain
        .set_task_advisor_session(task_id, Some(session_id.clone()))
        .await
    {
        tracing::warn!(module = "transport-http-transport-routes_task_advisor", task_id, error = %e, "failed to persist session_ids.advisor");
    }

    state
        .captain
        .persist_task_answer(
            task_id,
            &ask_id,
            &session_id,
            &body.message,
            &answer,
            &body.intent,
        )
        .await
        .map_err(|e| internal_error(e, "failed to persist advisor answer"))?;

    broadcast_task_update(&state, task_id).await;
    touch_workbench_activity(&state, item.workbench_id).await;

    if body.intent == "ask" {
        let config = state.settings.load_config();
        let notifier = crate::captain_notifier(&state, &config);
        let mut preview: String = answer.chars().take(200).collect();
        if answer.chars().count() > 200 {
            preview.push_str("...");
        }
        let msg = format!(
            "Advisor answered on <b>{}</b>: {}",
            global_infra::html::escape_html(&item.title),
            global_infra::html::escape_html(&preview),
        );
        notifier
            .notify_typed(
                &msg,
                api_types::NotifyLevel::Normal,
                api_types::NotificationKind::AdvisorAnswered {
                    item_id: task_id.to_string(),
                    title: item.title.clone(),
                },
                Some(&task_id.to_string()),
            )
            .await;
    }

    if body.intent == "reopen" || body.intent == "rework" || body.intent == "revise-plan" {
        let action_result = handle_advisor_action(
            &state,
            task_id,
            &session_key,
            &body.intent,
            &cwd,
            &workflow,
            &answer,
        )
        .await;

        if let Err(ref err_resp) = action_result {
            let error_msg = err_resp.1 .0.error.as_str();
            if let Err(e) = state
                .captain
                .persist_task_error(task_id, &ask_id, PENDING_SESSION, &body.message, error_msg)
                .await
            {
                tracing::error!(module = "transport-http-transport-routes_task_advisor", task_id, error = %e, "failed to persist advisor action error");
            }
            broadcast_task_update(&state, task_id).await;
        }

        return action_result;
    }

    Ok(Json(api_types::AdvisorResponse::Ask(
        api_types::AdvisorAskResponse {
            id: task_id,
            ask_id,
            message: body.message,
            answer,
            session_id,
        },
    )))
}

/// Run the advisor CC session with up to max_retries attempts.
/// Each failure is persisted to ask_history so it surfaces in the feed.
#[allow(clippy::too_many_arguments)]
async fn run_advisor_cc(
    state: &AppState,
    sessions: &::sessions::SessionsRuntime,
    session_key: &str,
    should_resume: bool,
    message: &str,
    item: &captain::Task,
    workflow: &settings::config::CaptainWorkflow,
    task_id: i64,
    ask_id: &str,
    cwd: &std::path::Path,
) -> Result<global_claude::CcResult<serde_json::Value>, ApiError> {
    let task_id_str = task_id.to_string();
    let timeline_text = state
        .captain
        .build_task_timeline_text(task_id)
        .await
        .map_err(|e| internal_error(e, "failed to build advisor timeline"))?;
    let prompt = build_advisor_prompt(item, &task_id_str, message, workflow, &timeline_text)
        .map_err(|e| internal_error(e, "failed to build advisor prompt"))?;

    let max_retries = workflow.agent.max_advisor_retries;
    let mut should_resume_attempt = should_resume;

    for attempt in 1..=max_retries {
        let result = if should_resume_attempt {
            sessions
                .follow_up(::sessions::SessionFollowUpRequest {
                    key: session_key.to_string(),
                    message: message.to_string(),
                    cwd: cwd.to_path_buf(),
                })
                .await
        } else {
            sessions
                .start_with_item(::sessions::SessionStartRequest {
                    key: session_key.to_string(),
                    prompt: prompt.clone(),
                    cwd: cwd.to_path_buf(),
                    model: Some(workflow.models.captain.clone()),
                    idle_ttl: workflow.agent.task_ask_idle_ttl_s,
                    call_timeout: workflow.agent.task_ask_timeout_s,
                    task_id: Some(task_id),
                    max_turns: None,
                })
                .await
        };

        match result {
            Ok(r) => return Ok(r),
            Err(e) => {
                let error_msg = e.to_string();
                tracing::error!(
                    module = "transport-http-transport-routes_task_advisor", task_id, attempt, max_retries = max_retries,
                    error = %error_msg, "advisor CC session failed"
                );

                sessions.close(session_key);
                if let Err(e) = state.captain.set_task_advisor_session(task_id, None).await {
                    tracing::warn!(module = "transport-http-transport-routes_task_advisor", task_id, error = %e, "failed to clear advisor session after retry error");
                }

                let display_msg = if attempt < max_retries {
                    format!("Attempt {attempt}/{max_retries} failed: {error_msg} — retrying…")
                } else {
                    format!("Failed after {max_retries} attempts: {error_msg}")
                };
                if let Err(e) = state
                    .captain
                    .persist_task_error(task_id, ask_id, PENDING_SESSION, message, &display_msg)
                    .await
                {
                    tracing::error!(module = "transport-http-transport-routes_task_advisor", task_id, error = %e, "failed to persist advisor retry error");
                }
                broadcast_task_update(state, task_id).await;

                should_resume_attempt = false;
            }
        }
    }

    Err(error_response(
        StatusCode::INTERNAL_SERVER_ERROR,
        "advisor session failed after all retries",
    ))
}

/// Handle reopen/rework via advisor synthesis.
async fn handle_advisor_action(
    state: &AppState,
    task_id: i64,
    session_key: &str,
    intent: &str,
    cwd: &std::path::Path,
    workflow: &settings::config::CaptainWorkflow,
    _initial_answer: &str,
) -> Result<Json<api_types::AdvisorResponse>, ApiError> {
    {
        let item = state
            .captain
            .load_task(task_id)
            .await
            .map_err(|e| internal_error(e, "failed to load task"))?
            .ok_or_else(|| {
                error_response(StatusCode::NOT_FOUND, &format!("item {task_id} not found"))
            })?;
        if !action_eligible(intent, &item.status()) {
            return Err(error_response(
                StatusCode::BAD_REQUEST,
                &format!("cannot {} from status {:?}", intent, item.status()),
            ));
        }
    }

    let sessions = state.sessions.clone();
    let synthesis_prompt = workflow
        .prompts
        .get("advisor_reopen_synthesis")
        .ok_or_else(|| {
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "advisor_reopen_synthesis prompt missing from captain-workflow.yaml",
            )
        })?
        .clone();

    let result = sessions
        .follow_up(::sessions::SessionFollowUpRequest {
            key: session_key.to_string(),
            message: synthesis_prompt,
            cwd: cwd.to_path_buf(),
        })
        .await
        .map_err(|e| internal_error(e, "advisor synthesis session failed"))?;
    let synthesized_feedback = result.text.clone();

    crate::runtime::task_sessions::close_ask_session(state, task_id).await;

    let config = state.settings.load_config();
    let notifier = crate::captain_notifier(state, &config);
    let mut item = state
        .captain
        .load_task(task_id)
        .await
        .map_err(|e| internal_error(e, "failed to load task"))?
        .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "item vanished during synthesis"))?;

    if !action_eligible(intent, &item.status()) {
        return Err(error_response(
            StatusCode::CONFLICT,
            &format!(
                "task status changed to {:?} during synthesis -- cannot {}",
                item.status(),
                intent
            ),
        ));
    }

    let previous_status = item.status();
    let old_session_id = item.session_ids.worker.clone();
    let mut reopen_event: Option<captain::TimelineEvent> = None;
    let mut follow_up_effects: Vec<EffectRequest> = Vec::new();

    if intent == "revise-plan" && item.status() == captain::ItemStatus::PlanReady {
        let existing_ctx = item.context.as_deref().unwrap_or("");
        item.context = Some(format!(
            "{existing_ctx}\n\n## Revision feedback\n{synthesized_feedback}"
        ));
        item.planning = true;
        let _ignored = captain::apply_transition(&mut item, captain::ItemStatus::Queued)
            .map_err(|e| internal_error(e, "failed to revise plan transition"))?;
        item.last_activity_at = Some(global_types::now_rfc3339());
        state
            .captain
            .write_task(&item)
            .await
            .map_err(|e| internal_error(e, "failed to save task"))?;
    } else if intent == "rework" {
        state
            .captain
            .rework_item(task_id, &synthesized_feedback)
            .await
            .map_err(|e| internal_error(e, "failed to rework task"))?;
    } else {
        let outcome = state
            .captain
            .reopen_item_from_human(&mut item, &synthesized_feedback, workflow, &notifier)
            .await
            .map_err(|e| internal_error(e, "failed to reopen task"))?;

        reopen_event = Some(captain::TimelineEvent {
            timestamp: global_types::now_rfc3339(),
            actor: "human".to_string(),
            summary: format!("Reopened from advisor: {synthesized_feedback}"),
            data: TimelineEventPayload::HumanReopen {
                content: synthesized_feedback.clone(),
                source: "advisor".to_string(),
                worker: item.worker.clone().unwrap_or_default(),
                session_id: item.session_ids.worker.clone().unwrap_or_default(),
                from: previous_status.as_str().to_string(),
                to: item.status().as_str().to_string(),
            },
        });

        if matches!(outcome, captain::ReopenOutcome::Reopened) {
            let truly_resumed =
                old_session_id.is_some() && old_session_id == item.session_ids.worker;
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
            follow_up_effects.push(EffectRequest::NotifyNormal {
                message: format!(
                    "\u{1f504} Reopened <b>{}</b>: {}",
                    global_infra::html::escape_html(&item.title),
                    global_infra::html::escape_html(&synthesized_feedback),
                ),
            });
        }
    }

    state.sessions.close(session_key);
    if let Err(e) = state.captain.set_task_advisor_session(task_id, None).await {
        tracing::warn!(module = "transport-http-transport-routes_task_advisor", task_id, error = %e, "failed to clear session_ids.advisor after action");
    }

    if let Some(event) = reopen_event.as_ref() {
        if matches!(item.status(), captain::ItemStatus::CaptainReviewing) {
            return Ok(Json(api_types::AdvisorResponse::Action(
                api_types::AdvisorActionResponse {
                    ok: true,
                    intent: intent.to_string(),
                    feedback: synthesized_feedback,
                },
            )));
        }

        let applied = state
            .captain
            .persist_task_transition_with_effects(
                &item,
                previous_status.as_str(),
                event,
                follow_up_effects,
            )
            .await
            .map_err(|e| internal_error(e, "failed to save advisor reopen transition"))?;
        if !applied {
            return Err(error_response(
                StatusCode::CONFLICT,
                "task changed concurrently while reopening from advisor",
            ));
        }
    }

    Ok(Json(api_types::AdvisorResponse::Action(
        api_types::AdvisorActionResponse {
            ok: true,
            intent: intent.to_string(),
            feedback: synthesized_feedback,
        },
    )))
}
