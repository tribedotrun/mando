//! Task ask route handlers — multi-turn Q&A sessions with worktree access.
use api_types::TimelineEventPayload;
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use captain::EffectRequest;

use crate::response::{
    broadcast_task_update, error_response, internal_error, resolve_task_cwd,
    touch_workbench_activity, ApiError,
};
use crate::AppState;

/// POST /api/tasks/ask (JSON or multipart with optional images)
///
/// First ask creates a new CC session in the task's worktree.
/// Follow-up asks resume the same session via `--resume`.
#[crate::instrument_api(method = "POST", path = "/api/tasks/ask")]
pub(crate) async fn post_task_ask(
    State(state): State<AppState>,
    request: axum::extract::Request,
) -> Result<Json<api_types::AskResponse>, ApiError> {
    let body = crate::image_upload::extract_ask(request).await?;
    let result = post_task_ask_inner(&state, &body).await;
    if result.is_err() {
        crate::image_upload::cleanup_saved_images(&body.saved_images).await;
    }
    result
}

async fn post_task_ask_inner(
    state: &AppState,
    body: &crate::image_upload::AskWithImages,
) -> Result<Json<api_types::AskResponse>, ApiError> {
    let id = body.id;
    let workflow = state.settings.load_captain_workflow();

    let item = state
        .captain
        .load_task(id)
        .await
        .map_err(|e| internal_error(e, "failed to load task"))?
        .ok_or_else(|| error_response(StatusCode::NOT_FOUND, &format!("item {id} not found")))?;

    let cwd = resolve_task_cwd(&item, state)?;

    let session_key = format!("task-ask:{id}");
    let sessions = state.sessions.clone();

    let mgr_has_session = sessions.has_session(&session_key);
    let task_has_session = item.session_ids.ask.is_some();
    let should_resume = mgr_has_session && task_has_session;

    if mgr_has_session && !task_has_session {
        tracing::info!(
            module = "transport-http-transport-routes_task_ask",
            task_id = id,
            "session_ids.ask cleared by lifecycle — closing stale session"
        );
        sessions.close(&session_key);
    } else if !mgr_has_session && task_has_session {
        tracing::warn!(
            module = "transport-http-transport-routes_task_ask",
            task_id = id,
            "stale session_ids.ask — manager has no session, clearing"
        );
        if let Err(e) = state.captain.set_task_ask_session(id, None).await {
            tracing::warn!(module = "transport-http-transport-routes_task_ask", task_id = id, error = %e, "failed to clear stale session_ids.ask");
        }
    }

    let ask_id = body
        .ask_id
        .clone()
        .unwrap_or_else(|| global_infra::uuid::Uuid::v4().to_string());

    const PENDING_SESSION: &str = "pending";

    state
        .captain
        .persist_task_question(id, &ask_id, PENDING_SESSION, &body.question)
        .await
        .map_err(|e| internal_error(e, "failed to persist ask question"))?;
    broadcast_task_update(state, id).await;

    let question_for_cc = if body.saved_images.is_empty() {
        body.question.clone()
    } else {
        format!(
            "{}{}",
            body.question,
            crate::image_upload::format_image_paths(&body.saved_images)
        )
    };

    let cc_result = if should_resume {
        sessions
            .follow_up(::sessions::SessionFollowUpRequest {
                key: session_key.clone(),
                message: question_for_cc.clone(),
                cwd: cwd.clone(),
            })
            .await
    } else {
        let prompt = state
            .captain
            .build_task_ask_initial_prompt(&item, &question_for_cc, &workflow)
            .await
            .map_err(|e| internal_error(e, "failed to build ask prompt"))?;

        sessions
            .start_with_item(::sessions::SessionStartRequest {
                key: session_key.clone(),
                prompt,
                cwd: cwd.clone(),
                model: Some(workflow.models.captain.clone()),
                idle_ttl: workflow.agent.task_ask_idle_ttl_s,
                call_timeout: workflow.agent.task_ask_timeout_s,
                task_id: Some(id),
                max_turns: None,
            })
            .await
    };

    match cc_result {
        Ok(result) => {
            let answer = result.text.clone();
            let session_id = result.session_id.clone();

            if !should_resume {
                if let Err(e) = state
                    .captain
                    .set_task_ask_session(id, Some(session_id.clone()))
                    .await
                {
                    tracing::warn!(module = "transport-http-transport-routes_task_ask", task_id = id, error = %e, "failed to persist session_ids.ask");
                }
            }

            state
                .captain
                .persist_task_answer(id, &ask_id, &session_id, &body.question, &answer, "ask")
                .await
                .map_err(|e| internal_error(e, "failed to persist ask answer"))?;

            if !body.saved_images.is_empty() {
                if let Err(e) = state
                    .captain
                    .append_task_images(id, &body.saved_images)
                    .await
                {
                    tracing::warn!(module = "transport-http-transport-routes_task_ask", task_id = id, error = ?e, "failed to persist ask images");
                }
            }

            broadcast_task_update(state, id).await;
            touch_workbench_activity(state, item.workbench_id).await;

            Ok(Json(api_types::AskResponse {
                id: Some(id),
                ask_id,
                question: Some(body.question.clone()),
                answer,
                session_id: Some(session_id),
                suggested_followups: None,
            }))
        }
        Err(e) => {
            let error_msg = e.to_string();
            tracing::error!(module = "transport-http-transport-routes_task_ask", task_id = id, error = %error_msg, "ask CC session failed");

            state.sessions.close(&session_key);
            if let Err(e) = state.captain.set_task_ask_session(id, None).await {
                tracing::warn!(module = "transport-http-transport-routes_task_ask", task_id = id, error = %e, "failed to clear ask session id");
            }

            if let Err(persist_err) = state
                .captain
                .persist_task_error(id, &ask_id, PENDING_SESSION, &body.question, &error_msg)
                .await
            {
                tracing::error!(module = "transport-http-transport-routes_task_ask", task_id = id, error = %persist_err, "failed to persist ask error");
            }

            broadcast_task_update(state, id).await;
            Err(error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "ask session failed",
            ))
        }
    }
}

/// POST /api/tasks/ask/end — end the ask session for a task.
#[crate::instrument_api(method = "POST", path = "/api/tasks/ask/end")]
pub(crate) async fn post_task_ask_end(
    State(state): State<AppState>,
    Json(body): Json<api_types::TaskIdRequest>,
) -> Result<Json<api_types::AskEndResponse>, ApiError> {
    let id = body.id;
    let session_key = format!("task-ask:{id}");

    state.sessions.close(&session_key);

    if let Err(e) = state.captain.set_task_ask_session(id, None).await {
        tracing::warn!(module = "transport-http-transport-routes_task_ask", task_id = id, error = %e, "failed to clear session_ids.ask on end");
    }

    let updated = state.captain.task_json(id).await.ok().flatten();
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
            id: Some(id),
            cleared_by: None,
        },
    )));
    touch_workbench_activity(&state, wb_id).await;

    Ok(Json(api_types::AskEndResponse {
        ok: true,
        ended: session_key,
    }))
}

/// POST /api/tasks/ask/reopen — synthesize Q&A into feedback and reopen.
///
/// Sends a follow-up to the active Q&A session asking it to produce a reopen
/// message, then closes the session and delegates to `action_contract::reopen_item`.
#[crate::instrument_api(method = "POST", path = "/api/tasks/ask/reopen")]
pub(crate) async fn post_task_ask_reopen(
    State(state): State<AppState>,
    Json(body): Json<api_types::TaskIdRequest>,
) -> Result<Json<api_types::AskReopenResponse>, ApiError> {
    use captain::ItemStatus;

    let id = body.id;
    let workflow = state.settings.load_captain_workflow();

    let item = state
        .captain
        .load_task(id)
        .await
        .map_err(|e| internal_error(e, "failed to load task"))?
        .ok_or_else(|| error_response(StatusCode::NOT_FOUND, &format!("item {id} not found")))?;

    let can_ask_reopen = matches!(
        item.status(),
        ItemStatus::AwaitingReview | ItemStatus::Escalated
    );
    if !can_ask_reopen {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            &format!("cannot reopen from Q&A in status {:?}", item.status()),
        ));
    }

    let history = state
        .captain
        .task_ask_history(id)
        .await
        .map_err(|e| internal_error(e, "failed to load ask history"))?;
    if history.is_empty() {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "no Q&A history to synthesize — ask at least one question first",
        ));
    }

    let cwd = resolve_task_cwd(&item, &state)?;
    let session_key = format!("task-ask:{id}");
    let sessions = state.sessions.clone();

    if item.session_ids.ask.is_none() || !sessions.has_session(&session_key) {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "no active Q&A session — use the standard reopen action instead",
        ));
    }

    let synthesis_prompt = workflow
        .prompts
        .get("task_ask_reopen_synthesis")
        .ok_or_else(|| {
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                "task_ask_reopen_synthesis prompt missing from captain-workflow.yaml",
            )
        })?
        .clone();

    let result = sessions
        .follow_up(::sessions::SessionFollowUpRequest {
            key: session_key.clone(),
            message: synthesis_prompt,
            cwd: cwd.clone(),
        })
        .await
        .map_err(|e| internal_error(e, "ask synthesis session failed"))?;
    let synthesized_feedback = result.text.clone();

    let config = state.settings.load_config();
    let notifier = crate::captain_notifier(&state, &config);
    let mut item = state
        .captain
        .load_task(id)
        .await
        .map_err(|e| internal_error(e, "failed to load task"))?
        .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "item vanished during synthesis"))?;
    let previous_status = item.status();

    let old_session_id = item.session_ids.worker.clone();
    let outcome = state
        .captain
        .reopen_item_from_human(&mut item, &synthesized_feedback, &workflow, &notifier)
        .await
        .map_err(|e| internal_error(e, "failed to reopen task"))?;

    crate::runtime::task_sessions::close_ask_session(&state, id).await;

    let event = captain::TimelineEvent {
        timestamp: global_types::now_rfc3339(),
        actor: "human".to_string(),
        summary: format!("Reopened from Q&A: {}", &synthesized_feedback),
        data: TimelineEventPayload::HumanReopen {
            content: synthesized_feedback.clone(),
            source: "ask-reopen".to_string(),
            worker: item.worker.clone().unwrap_or_default(),
            session_id: item.session_ids.worker.clone().unwrap_or_default(),
            from: previous_status.as_str().to_string(),
            to: item.status().as_str().to_string(),
        },
    };
    let mut effects: Vec<EffectRequest> = Vec::new();

    if matches!(outcome, captain::ReopenOutcome::Reopened) {
        let truly_resumed = old_session_id.is_some() && old_session_id == item.session_ids.worker;
        let (evt_payload, evt_summary) = if truly_resumed {
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
            .emit_task_timeline_event(&item, &evt_summary, evt_payload)
            .await;

        let msg = format!(
            "\u{1f504} Reopened <b>{}</b> from Q&A",
            global_infra::html::escape_html(&item.title),
        );
        effects.push(EffectRequest::NotifyNormal { message: msg });
    }

    if matches!(outcome, captain::ReopenOutcome::CaptainReviewing) {
        return Ok(Json(api_types::AskReopenResponse {
            ok: true,
            feedback: synthesized_feedback,
        }));
    }

    let applied = state
        .captain
        .persist_task_transition_with_effects(&item, previous_status.as_str(), &event, effects)
        .await
        .map_err(|e| internal_error(e, "failed to save task"))?;
    if !applied {
        return Err(error_response(
            StatusCode::CONFLICT,
            "task changed concurrently while reopening from Q&A",
        ));
    }

    Ok(Json(api_types::AskReopenResponse {
        ok: true,
        feedback: synthesized_feedback,
    }))
}
