//! POST /api/tasks/{id}/clarify — unified clarification answer endpoint.

use std::panic::AssertUnwindSafe;
use std::sync::Arc;

use api_types::TimelineEventPayload;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use futures_util::FutureExt;

use captain::ClarifierStatus;

use crate::response::{error_response, internal_error, ApiError};
use crate::AppState;

/// POST /api/tasks/{id}/clarify (JSON or multipart with optional images).
///
/// Default (`?wait=true` or absent): synchronous — the response carries the
/// reclarifier outcome. Used by `mando todo input` and the Telegram bot,
/// both of which render the immediate result.
///
/// `?wait=false`: returns as soon as the answer is committed and the
/// follow-up CC reclarify call is spawned. The renderer uses this to keep
/// the clarification form responsive; the next state arrives via SSE.
#[crate::instrument_api(method = "POST", path = "/api/tasks/{id}/clarify")]
pub(crate) async fn post_task_clarify(
    State(state): State<AppState>,
    Path(api_types::TaskIdParams { id }): Path<api_types::TaskIdParams>,
    Query(q): Query<api_types::ClarifyQuery>,
    request: axum::extract::Request,
) -> Result<Json<api_types::ClarifyResponse>, ApiError> {
    let body = crate::image_upload_ext::extract_clarify(request).await?;
    let wait = q.wait.unwrap_or(true);
    let result = post_task_clarify_inner(&state, id, &body, wait).await;
    if let Err(ref e) = result {
        if is_early_clarify_error(e) {
            crate::image_upload::cleanup_saved_images(&body.saved_images).await;
        }
    }
    result
}

fn is_early_clarify_error(err: &ApiError) -> bool {
    matches!(
        err.0,
        StatusCode::BAD_REQUEST | StatusCode::NOT_FOUND | StatusCode::CONFLICT
    )
}

async fn post_task_clarify_inner(
    state: &AppState,
    id: i64,
    body: &crate::image_upload::ClarifyWithImages,
    wait: bool,
) -> Result<Json<api_types::ClarifyResponse>, ApiError> {
    let mut answer = if let Some(ref answers) = body.answers {
        if answers.is_empty() {
            return Err(error_response(
                StatusCode::BAD_REQUEST,
                "answers array must not be empty",
            ));
        }
        answers
            .iter()
            .enumerate()
            .map(|(i, a)| format!("Q{}: {}\nA{}: {}", i + 1, a.question, i + 1, a.answer))
            .collect::<Vec<_>>()
            .join("\n\n")
    } else if let Some(ref text) = body.answer {
        let trimmed = text.trim().to_string();
        if trimmed.is_empty() {
            return Err(error_response(
                StatusCode::BAD_REQUEST,
                "answer text must not be empty",
            ));
        }
        trimmed
    } else {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "either 'answer' or 'answers' is required",
        ));
    };

    if !body.saved_images.is_empty() {
        answer = format!(
            "{}{}",
            answer,
            crate::image_upload::format_image_paths(&body.saved_images)
        );
    }

    let mut item = state
        .captain
        .load_task(id)
        .await
        .map_err(|e| internal_error(e, "failed to load task"))?
        .ok_or_else(|| error_response(StatusCode::NOT_FOUND, &format!("task {id} not found")))?;

    if item.status() != captain::ItemStatus::NeedsClarification {
        return Err(error_response(
            StatusCode::CONFLICT,
            &format!(
                "task must be in needs-clarification, got {:?}",
                item.status()
            ),
        ));
    }

    let new_context = state
        .captain
        .append_task_note(item.context.as_deref(), "Human answer", &answer)
        .ok_or_else(|| {
            error_response(
                StatusCode::BAD_REQUEST,
                "answer must contain non-empty text to produce a note",
            )
        })?;
    item.context = Some(new_context);

    // Commit the needs-clarification -> clarifying transition before the
    // inline reclarifier runs. `persist_resume_clarifier` owns the
    // transition: it calls `apply_transition` on the in-memory item,
    // clears the stale clarifier session id, then writes the row with a
    // conditional UPDATE guarded on the needs-clarification predecessor.
    // Doing the transition + write here as well would trip the lifecycle
    // guard on the runtime-level re-apply (Clarifying -> Clarifying is
    // illegal).
    if let Err(e) = state.captain.persist_resume_clarifier(&mut item).await {
        return Err(internal_error(
            e,
            "failed to advance task to clarifying for inline re-clarification",
        ));
    }

    if !body.saved_images.is_empty() {
        if let Err(e) = state
            .captain
            .append_task_images(id, &body.saved_images)
            .await
        {
            tracing::warn!(module = "transport-http-transport-routes_clarifier", task_id = id, error = ?e, "failed to persist clarify images");
        }
    }

    let _ignored = state
        .captain
        .emit_task_timeline_event(
            &item,
            &format!("Human answered: {answer}"),
            TimelineEventPayload::HumanAnswered {
                answer: answer.clone(),
            },
        )
        .await;

    let wf = state.settings.load_captain_workflow();

    if !wait {
        // Async path: ack the submit and run the long CC reclarify call on
        // the daemon's task tracker. The renderer uses this to keep the
        // clarification form unblocked while the clarifier thinks.
        //
        // Drain timeline outboxes so the `HumanAnswered` event lands in
        // the DB before the broadcast — without this, the renderer feed
        // sees a stale window between status change and event delivery.
        if let Err(e) = state.captain.drain_pending_lifecycle_effects().await {
            tracing::warn!(
                module = "clarifier",
                task_id = id,
                error = %e,
                "failed to drain timeline effects before async clarify ack"
            );
        }
        // Broadcast the NeedsClarification -> Clarifying transition so the
        // SSE-driven renderer cache flips immediately.
        state.captain.broadcast_task_update(id).await;

        spawn_inline_reclarify(state.clone(), item.clone(), answer, wf);

        return Ok(Json(api_types::ClarifyResponse {
            ok: true,
            status: api_types::ClarifyOutcome::Clarifying,
            context: item.context.clone(),
            questions: None,
            session_id: item.session_ids.clarifier.clone(),
            error: None,
        }));
    }

    match state
        .captain
        .answer_and_reclarify(&item, &answer, &wf)
        .await
    {
        Ok(result) => {
            let response_outcome = result.status.clone();
            let response_context = Some(result.context.clone());
            let response_session_id = result.session_id.clone();
            let questions = match result.questions.as_ref() {
                Some(questions) => Some(
                    serde_json::from_value(serde_json::to_value(questions).map_err(|e| {
                        internal_error(e, "failed to serialize clarifier questions")
                    })?)
                    .map_err(|e| internal_error(e, "failed to serialize clarifier questions"))?,
                ),
                None => None,
            };

            state
                .captain
                .apply_clarifier_result(&mut item, result, &wf)
                .await
                .map_err(|e| internal_error(e, "failed to apply clarification result"))?;

            let status = match item.status() {
                captain::ItemStatus::Queued => api_types::ClarifyOutcome::Ready,
                captain::ItemStatus::NeedsClarification => api_types::ClarifyOutcome::Clarifying,
                captain::ItemStatus::CaptainReviewing => api_types::ClarifyOutcome::Escalate,
                captain::ItemStatus::CompletedNoPr => api_types::ClarifyOutcome::Answered,
                _ => match response_outcome {
                    ClarifierStatus::Ready => api_types::ClarifyOutcome::Ready,
                    ClarifierStatus::Clarifying => api_types::ClarifyOutcome::Clarifying,
                    ClarifierStatus::Escalate => api_types::ClarifyOutcome::Escalate,
                    ClarifierStatus::Answered => api_types::ClarifyOutcome::Answered,
                },
            };

            state.captain.broadcast_task_update(id).await;

            Ok(Json(api_types::ClarifyResponse {
                ok: true,
                status,
                context: item.context.clone().or(response_context),
                questions,
                session_id: response_session_id,
                error: None,
            }))
        }
        Err(e) => {
            tracing::warn!(
                module = "clarifier",
                task_id = id,
                error = %e,
                "inline re-clarification failed — rolling task back to needs-clarification"
            );
            let api_error_status = e
                .downcast_ref::<global_claude::CcError>()
                .and_then(|cc| cc.api_error_status().and_then(|s| u16::try_from(s).ok()));
            let message = format!("{e}");
            let session_id = item.session_ids.clarifier.clone();
            if let Err(rollback_err) = state
                .captain
                .rollback_clarifier_after_failure(
                    &mut item,
                    session_id.as_deref(),
                    api_error_status,
                    &message,
                )
                .await
            {
                tracing::warn!(
                    module = "clarifier",
                    task_id = id,
                    error = %rollback_err,
                    "failed to roll task back to needs-clarification after CC error"
                );
            }
            state.captain.broadcast_task_update(id).await;
            // Re-clarification failed; the task is back at needs-clarification
            // and the client can re-fetch via GET /api/tasks/{id} or retry
            // via POST /api/tasks/{id}/clarify.
            Err(internal_error(e, "inline re-clarification failed"))
        }
    }
}

/// Run the same answer_and_reclarify -> apply / rollback -> broadcast
/// pipeline that the synchronous path runs, but on the daemon's
/// task_tracker so the HTTP response can return immediately. A panic
/// inside the spawn rolls the task back to NeedsClarification rather
/// than stranding it in Clarifying until startup reconciliation runs.
fn spawn_inline_reclarify(
    state: AppState,
    item: captain::Task,
    answer: String,
    wf: Arc<settings::CaptainWorkflow>,
) {
    let id = item.id;
    state.task_tracker.spawn(async move {
        let mut item = item;
        let result = AssertUnwindSafe(async {
            match state
                .captain
                .answer_and_reclarify(&item, &answer, &wf)
                .await
            {
                Ok(result) => {
                    if let Err(e) = state
                        .captain
                        .apply_clarifier_result(&mut item, result, &wf)
                        .await
                    {
                        tracing::warn!(
                            module = "clarifier",
                            task_id = id,
                            error = %e,
                            "failed to apply async clarification result — rolling task back to needs-clarification"
                        );
                        // Mirror the CC-error path: without rollback the task stays
                        // in Clarifying with no caller to retry, and startup
                        // reconciliation only catches daemon-crash strandings.
                        let message = format!("{e}");
                        let session_id = item.session_ids.clarifier.clone();
                        if let Err(rollback_err) = state
                            .captain
                            .rollback_clarifier_after_failure(
                                &mut item,
                                session_id.as_deref(),
                                None,
                                &message,
                            )
                            .await
                        {
                            tracing::warn!(
                                module = "clarifier",
                                task_id = id,
                                error = %rollback_err,
                                "failed to roll task back to needs-clarification after async apply error"
                            );
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        module = "clarifier",
                        task_id = id,
                        error = %e,
                        "async re-clarification failed — rolling task back to needs-clarification"
                    );
                    let api_error_status = e
                        .downcast_ref::<global_claude::CcError>()
                        .and_then(|cc| cc.api_error_status().and_then(|s| u16::try_from(s).ok()));
                    let message = format!("{e}");
                    let session_id = item.session_ids.clarifier.clone();
                    if let Err(rollback_err) = state
                        .captain
                        .rollback_clarifier_after_failure(
                            &mut item,
                            session_id.as_deref(),
                            api_error_status,
                            &message,
                        )
                        .await
                    {
                        tracing::warn!(
                            module = "clarifier",
                            task_id = id,
                            error = %rollback_err,
                            "failed to roll task back to needs-clarification after async CC error"
                        );
                    }
                }
            }
        })
        .catch_unwind()
        .await;

        if let Err(panic) = result {
            tracing::error!(
                module = "clarifier",
                task_id = id,
                "async re-clarification panicked: {:?}",
                panic
            );
            let session_id = item.session_ids.clarifier.clone();
            if let Err(rollback_err) = state
                .captain
                .rollback_clarifier_after_failure(
                    &mut item,
                    session_id.as_deref(),
                    None,
                    "async re-clarification panicked",
                )
                .await
            {
                tracing::warn!(
                    module = "clarifier",
                    task_id = id,
                    error = %rollback_err,
                    "failed to roll task back to needs-clarification after panic"
                );
            }
        }

        state.captain.broadcast_task_update(id).await;
    });
}
