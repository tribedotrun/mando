//! POST /api/tasks/{id}/clarify — unified clarification answer endpoint.

use api_types::TimelineEventPayload;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;

use captain::ClarifierStatus;

use crate::response::{error_response, internal_error, ApiError};
use crate::AppState;

/// POST /api/tasks/{id}/clarify (JSON or multipart with optional images)
#[crate::instrument_api(method = "POST", path = "/api/tasks/{id}/clarify")]
pub(crate) async fn post_task_clarify(
    State(state): State<AppState>,
    Path(api_types::TaskIdParams { id }): Path<api_types::TaskIdParams>,
    request: axum::extract::Request,
) -> Result<Json<api_types::ClarifyResponse>, ApiError> {
    let body = crate::image_upload_ext::extract_clarify(request).await?;
    let result = post_task_clarify_inner(&state, id, &body).await;
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
    state
        .captain
        .write_task(&item)
        .await
        .map_err(|e| internal_error(e, "failed to save clarification answer"))?;

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

    // Commit the needs-clarification -> clarifying transition *before* running
    // the inline reclarifier; the subsequent apply_clarifier_result path
    // expects the task to already be in `clarifying` when it reads the row.
    if let Err(e) = state.captain.persist_resume_clarifier(&mut item).await {
        return Err(internal_error(
            e,
            "failed to advance task to clarifying for inline re-clarification",
        ));
    }

    let wf = state.settings.load_captain_workflow();
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

            let status_str = match item.status() {
                captain::ItemStatus::Queued => "ready",
                captain::ItemStatus::NeedsClarification => "clarifying",
                captain::ItemStatus::CaptainReviewing => "escalate",
                captain::ItemStatus::CompletedNoPr => "answered",
                _ => match response_outcome {
                    ClarifierStatus::Ready => "ready",
                    ClarifierStatus::Clarifying => "clarifying",
                    ClarifierStatus::Escalate => "escalate",
                    ClarifierStatus::Answered => "answered",
                },
            };

            state.captain.broadcast_task_update(id).await;

            Ok(Json(api_types::ClarifyResponse {
                ok: true,
                status: status_str.to_string(),
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
