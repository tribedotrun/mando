//! POST /api/tasks/{id}/clarify — unified clarification answer endpoint.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use mando_captain::runtime::clarifier::ClarifierStatus;

use crate::response::{error_response, internal_error};
use crate::AppState;

#[derive(Deserialize)]
pub(crate) struct QuestionAnswer {
    question: String,
    answer: String,
}

#[derive(Deserialize)]
pub(crate) struct ClarifyBody {
    /// Structured per-question answers (from Electron UI).
    #[serde(default)]
    pub answers: Option<Vec<QuestionAnswer>>,
    /// Flat text answer (from Telegram / CLI).
    #[serde(default)]
    pub answer: Option<String>,
}

/// POST /api/tasks/{id}/clarify — provide human answer and re-clarify inline.
pub(crate) async fn post_task_clarify(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(body): Json<ClarifyBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let answer = if let Some(answers) = body.answers {
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
    } else if let Some(text) = body.answer {
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

    // Load the task and validate status.
    let (item, pool) = {
        let store = state.task_store.read().await;
        let item = store
            .find_by_id(id)
            .await
            .map_err(internal_error)?
            .ok_or_else(|| {
                error_response(StatusCode::NOT_FOUND, &format!("task {id} not found"))
            })?;

        if item.status != mando_types::task::ItemStatus::NeedsClarification {
            return Err(error_response(
                StatusCode::CONFLICT,
                &format!("task must be in needs-clarification, got {:?}", item.status),
            ));
        }

        // Append human answer to context.
        let new_context = mando_captain::runtime::task_notes::append_tagged_note(
            item.context.as_deref(),
            "Human answer",
            &answer,
        )
        .expect("validated answer should always produce a note");
        store
            .update(id, |t| {
                t.context = Some(new_context.clone());
            })
            .await
            .map_err(internal_error)?;

        let mut updated = item;
        updated.context = Some(new_context);
        let pool = store.pool().clone();
        (updated, pool)
    };

    // Emit HumanAnswered timeline event.
    let _ = mando_captain::runtime::timeline_emit::emit_for_task(
        &item,
        mando_types::timeline::TimelineEventType::HumanAnswered,
        &format!("Human answered: {answer}"),
        json!({"answer": &answer}),
        &pool,
    )
    .await;

    // Run inline re-clarification.
    let wf = state.captain_workflow.load_full();
    let cfg = state.config.load_full();
    match mando_captain::runtime::clarifier::answer_and_reclarify(&item, &answer, &wf, &cfg, &pool)
        .await
    {
        Ok(result) => {
            // Build session_ids JSON preserving existing fields, updating clarifier.
            let sids = json!({
                "worker": item.session_ids.worker,
                "review": item.session_ids.review,
                "clarifier": result.session_id.as_deref().or(item.session_ids.clarifier.as_deref()),
                "merge": item.session_ids.merge,
            });

            let store = state.task_store.read().await;
            let status_str = match result.status {
                ClarifierStatus::Ready => {
                    mando_captain::runtime::dashboard::force_update_task(
                        &store,
                        id,
                        &json!({
                            "status": "queued",
                            "context": result.context,
                            "session_ids": sids,
                        }),
                    )
                    .await
                    .map_err(internal_error)?;

                    let _ = mando_captain::runtime::timeline_emit::emit_for_task(
                        &item,
                        mando_types::timeline::TimelineEventType::ClarifyResolved,
                        "Clarification complete, ready for work",
                        json!({"session_id": result.session_id}),
                        &pool,
                    )
                    .await;
                    "ready"
                }
                ClarifierStatus::Clarifying => {
                    mando_captain::runtime::dashboard::force_update_task(
                        &store,
                        id,
                        &json!({
                            "status": "needs-clarification",
                            "context": result.context,
                            "session_ids": sids,
                        }),
                    )
                    .await
                    .map_err(internal_error)?;

                    let _ = mando_captain::runtime::timeline_emit::emit_for_task(
                        &item,
                        mando_types::timeline::TimelineEventType::ClarifyQuestion,
                        "Still needs clarification",
                        json!({"session_id": result.session_id, "questions": result.questions}),
                        &pool,
                    )
                    .await;
                    "clarifying"
                }
                ClarifierStatus::Escalate => {
                    mando_captain::runtime::dashboard::force_update_task(
                        &store,
                        id,
                        &json!({
                            "status": "captain-reviewing",
                            "context": result.context,
                            "captain_review_trigger": "clarifier_fail",
                            "session_ids": sids,
                        }),
                    )
                    .await
                    .map_err(internal_error)?;
                    "escalate"
                }
            };

            state.bus.send(
                mando_types::BusEvent::Tasks,
                Some(json!({"action": "clarify", "id": id})),
            );

            Ok(Json(json!({
                "ok": true,
                "status": status_str,
                "context": result.context,
                "questions": result.questions,
                "session_id": result.session_id,
            })))
        }
        Err(e) => {
            // LLM failed — keep the human's answer in context but stay in
            // needs-clarification so the human can retry or captain can
            // pick it up on next tick. Return HTTP 500 so clients can
            // distinguish a real error from a successful clarification.
            tracing::warn!(
                module = "clarifier",
                task_id = id,
                error = %e,
                "inline re-clarification failed — answer saved, status unchanged"
            );
            state.bus.send(
                mando_types::BusEvent::Tasks,
                Some(json!({"action": "answer", "id": id})),
            );
            let questions: Option<serde_json::Value> =
                match mando_db::queries::timeline::latest_clarifier_questions(&pool, id).await {
                    Ok(q) => q,
                    Err(tl_err) => {
                        tracing::warn!(
                            module = "clarifier",
                            task_id = id,
                            error = %tl_err,
                            "failed to fetch questions for error response"
                        );
                        None
                    }
                };
            let body = json!({
                "ok": false,
                "status": "needs-clarification",
                "context": item.context,
                "questions": questions,
                "error": e.to_string(),
            });
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(body)))
        }
    }
}
