//! Action-path helpers for the task advisor route: synthesize the reopen /
//! rework / revise-plan message via a single CC call and apply the resulting
//! transition. Kept separate so `routes_task_advisor.rs` stays under the
//! file-length limit.

use api_types::TimelineEventPayload;
use axum::http::StatusCode;
use axum::Json;
use captain::EffectRequest;

use super::routes_task_advisor_helpers::{
    action_eligible, build_advisor_action_prompt, build_advisor_synthesis_prompt,
};
use crate::response::{broadcast_task_update, error_response, internal_error, ApiError};
use crate::AppState;

const PENDING_SESSION: &str = "pending";

/// Entry point for the `reopen` / `rework` / `revise-plan` branch of the
/// advisor endpoint. Runs one CC call for synthesis and applies the
/// transition. Persists a `persist_task_error` entry if the call fails.
#[allow(clippy::too_many_arguments)]
pub(crate) async fn post_task_advisor_action(
    state: &AppState,
    sessions: &::sessions::SessionsRuntime,
    session_key: &str,
    should_resume: bool,
    item: &captain::Task,
    workflow: &settings::CaptainWorkflow,
    task_id: i64,
    cwd: &std::path::Path,
    ask_id: &str,
    intent: &str,
    message: &str,
) -> Result<Json<api_types::AdvisorResponse>, ApiError> {
    let action_result = handle_advisor_action(
        state,
        sessions,
        session_key,
        should_resume,
        item,
        workflow,
        task_id,
        cwd,
        intent,
        message,
    )
    .await;

    if let Err(ref err_resp) = action_result {
        let error_msg = err_resp.1 .0.error.as_str();
        if let Err(e) = state
            .captain
            .persist_task_error(task_id, ask_id, PENDING_SESSION, message, error_msg)
            .await
        {
            tracing::error!(module = "transport-http-transport-routes_task_advisor_action", task_id, error = %e, "failed to persist advisor action error");
        }
        broadcast_task_update(state, task_id).await;
    } else {
        crate::response::touch_workbench_activity(state, item.workbench_id).await;
    }

    action_result
}

/// Synthesize a single action-message via CC and apply the reopen / rework /
/// revise-plan transition. Runs exactly one CC call: `follow_up` when the
/// advisor session from a prior Ask is still live, `start_with_item` with
/// the full-context direct prompt otherwise.
#[allow(clippy::too_many_arguments)]
async fn handle_advisor_action(
    state: &AppState,
    sessions: &::sessions::SessionsRuntime,
    session_key: &str,
    should_resume: bool,
    item: &captain::Task,
    workflow: &settings::CaptainWorkflow,
    task_id: i64,
    cwd: &std::path::Path,
    intent: &str,
    message: &str,
) -> Result<Json<api_types::AdvisorResponse>, ApiError> {
    if !action_eligible(intent, &item.status()) {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            &format!("cannot {} from status {:?}", intent, item.status()),
        ));
    }

    let synthesized_feedback = run_advisor_action_cc(
        state,
        sessions,
        session_key,
        should_resume,
        item,
        workflow,
        task_id,
        cwd,
        intent,
        message,
    )
    .await?;

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
                from: previous_status.into(),
                to: item.status().into(),
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

    crate::runtime::task_sessions::clear_advisor_session(state, task_id).await;

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

/// Single CC call that produces the synthesized action message.
#[allow(clippy::too_many_arguments)]
async fn run_advisor_action_cc(
    state: &AppState,
    sessions: &::sessions::SessionsRuntime,
    session_key: &str,
    should_resume: bool,
    item: &captain::Task,
    workflow: &settings::CaptainWorkflow,
    task_id: i64,
    cwd: &std::path::Path,
    intent: &str,
    message: &str,
) -> Result<String, ApiError> {
    let result = if should_resume {
        let synthesis_prompt = build_advisor_synthesis_prompt(intent, workflow)
            .map_err(|e| internal_error(e, "failed to build advisor synthesis prompt"))?;
        sessions
            .follow_up(::sessions::SessionFollowUpRequest {
                key: session_key.to_string(),
                message: synthesis_prompt,
                cwd: cwd.to_path_buf(),
            })
            .await
    } else {
        let task_id_str = task_id.to_string();
        let timeline_text = state
            .captain
            .build_task_timeline_text(task_id)
            .await
            .map_err(|e| internal_error(e, "failed to build advisor timeline"))?;
        let prompt = build_advisor_action_prompt(
            item,
            &task_id_str,
            message,
            intent,
            workflow,
            &timeline_text,
        )
        .map_err(|e| internal_error(e, "failed to build advisor action prompt"))?;

        sessions
            .start_with_item(::sessions::SessionStartRequest {
                key: session_key.to_string(),
                prompt,
                cwd: cwd.to_path_buf(),
                model: Some(workflow.models.captain.clone()),
                idle_ttl: workflow.agent.task_ask_idle_ttl_s,
                call_timeout: workflow.agent.task_ask_timeout_s,
                task_id: Some(task_id),
                max_turns: None,
            })
            .await
    };

    let result = result.map_err(|e| internal_error(e, "advisor synthesis session failed"))?;

    if !should_resume {
        if let Err(e) = state
            .captain
            .set_task_advisor_session(task_id, Some(result.session_id.clone()))
            .await
        {
            tracing::warn!(module = "transport-http-transport-routes_task_advisor_action", task_id, error = %e, "failed to persist session_ids.advisor");
        }
    }

    Ok(result.text)
}
