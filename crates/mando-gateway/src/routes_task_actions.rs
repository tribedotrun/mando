//! Task lifecycle-action route handlers (accept, cancel, reopen, rework, handoff).

use std::future::Future;

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::response::{error_response, internal_error, touch_workbench_activity};
use crate::AppState;

#[derive(Deserialize)]
pub(crate) struct IdBody {
    pub id: i64,
}

/// Shared wrapper for simple task actions that take a task store and an id,
/// return `anyhow::Result<()>`, and emit a `Tasks` bus event on success.
async fn simple_task_action<Fut>(
    state: &AppState,
    id: i64,
    work: Fut,
) -> Result<Json<Value>, (StatusCode, Json<Value>)>
where
    Fut: Future<Output = anyhow::Result<()>>,
{
    work.await
        .map_err(|e| internal_error(e, "task action failed"))?;
    let store = state.task_store.read().await;
    let updated = store
        .find_by_id(id)
        .await
        .ok()
        .flatten()
        .map(|t| serde_json::to_value(&t).unwrap());
    let wb_id = updated
        .as_ref()
        .and_then(|v| v.get("workbench_id"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    drop(store);
    state.bus.send(
        mando_types::BusEvent::Tasks,
        Some(json!({"action": "updated", "item": updated, "id": id})),
    );
    touch_workbench_activity(state, wb_id).await;
    Ok(Json(json!({"ok": true})))
}

/// POST /api/tasks/accept
pub(crate) async fn post_task_accept(
    State(state): State<AppState>,
    Json(body): Json<IdBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let id = body.id;
    let store = state.task_store.read().await;
    simple_task_action(
        &state,
        id,
        mando_captain::runtime::dashboard::accept_item(&store, id),
    )
    .await
}

/// POST /api/tasks/cancel
pub(crate) async fn post_task_cancel(
    State(state): State<AppState>,
    Json(body): Json<IdBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let id = body.id;
    let store = state.task_store.read().await;
    let pool = state.db.pool();
    simple_task_action(
        &state,
        id,
        mando_captain::runtime::dashboard::cancel_item(&store, id, pool),
    )
    .await
}

/// POST /api/tasks/reopen (JSON or multipart with optional images)
pub(crate) async fn post_task_reopen(
    State(state): State<AppState>,
    request: axum::extract::Request,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
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
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let id = body.id;
    let config = state.config.load_full();
    let workflow = state.captain_workflow.load_full();
    let notifier = crate::captain_notifier(state, &config);
    let store = state.task_store.write().await;
    let mut item = store
        .find_by_id(id)
        .await
        .map_err(|e| internal_error(e, "failed to load task"))?
        .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "item not found"))?;

    // Append uploaded images to the task before reopening.
    if !body.saved_images.is_empty() {
        let joined = body.saved_images.join(",");
        item.images = Some(match item.images.take() {
            Some(existing) if !existing.is_empty() => format!("{existing},{joined}"),
            _ => joined,
        });
    }

    // Close any active ask session — the worker will modify the codebase.
    crate::routes_task_ask::close_ask_session(state, id).await;

    let old_session_id = item.session_ids.worker.clone();
    let outcome = mando_captain::runtime::action_contract::reopen_item(
        &mut item,
        "human",
        &body.feedback,
        &config,
        &workflow,
        &notifier,
        store.pool(),
        true,
    )
    .await
    .map_err(|e| internal_error(e, "failed to reopen task"))?;
    store
        .write_task(&item)
        .await
        .map_err(|e| internal_error(e, "failed to save task"))?;

    state.bus.send(
        mando_types::BusEvent::Tasks,
        Some(json!({"action": "updated", "item": serde_json::to_value(&item).unwrap(), "id": id})),
    );
    touch_workbench_activity(state, item.workbench_id).await;

    let summary = match outcome {
        mando_captain::runtime::action_contract::ReopenOutcome::QueuedFallback => {
            if body.feedback.is_empty() {
                "Reopened — queued for fresh work".to_string()
            } else {
                format!("Reopened — queued for fresh work: {}", body.feedback)
            }
        }
        mando_captain::runtime::action_contract::ReopenOutcome::CaptainReviewing => {
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
    let _ = mando_captain::runtime::timeline_emit::emit_for_task(
        &item,
        mando_types::timeline::TimelineEventType::HumanReopen,
        &summary,
        json!({
            "content": &body.feedback,
            "worker": item.worker,
            "session_id": item.session_ids.worker,
        }),
        store.pool(),
    )
    .await;

    if matches!(
        outcome,
        mando_captain::runtime::action_contract::ReopenOutcome::Reopened
    ) {
        // Emit SessionResumed only when the session was truly resumed (same
        // session_id). If reopen_worker fell back to clean_and_spawn_fresh the
        // session_id changes and we emit WorkerSpawned instead.
        let truly_resumed = old_session_id.is_some() && old_session_id == item.session_ids.worker;
        let (evt, summary) = if truly_resumed {
            (
                mando_types::timeline::TimelineEventType::SessionResumed,
                format!("Resumed {}", item.worker.as_deref().unwrap_or("worker")),
            )
        } else {
            (
                mando_types::timeline::TimelineEventType::WorkerSpawned,
                format!("Spawned {}", item.worker.as_deref().unwrap_or("worker")),
            )
        };
        let _ = mando_captain::runtime::timeline_emit::emit_for_task(
            &item,
            evt,
            &summary,
            json!({
                "worker": item.worker,
                "session_id": item.session_ids.worker,
            }),
            store.pool(),
        )
        .await;

        let msg = if body.feedback.is_empty() {
            format!(
                "\u{1f504} Reopened <b>{}</b>",
                mando_shared::telegram_format::escape_html(&item.title)
            )
        } else {
            format!(
                "\u{1f504} Reopened <b>{}</b>: {}",
                mando_shared::telegram_format::escape_html(&item.title),
                mando_shared::telegram_format::escape_html(&body.feedback)
            )
        };
        notifier.normal(&msg).await;
    }

    Ok(Json(json!({"ok": true})))
}

/// POST /api/tasks/rework (JSON or multipart with optional images)
pub(crate) async fn post_task_rework(
    State(state): State<AppState>,
    request: axum::extract::Request,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
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
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let id = body.id;

    // Close any active ask session — the worktree will be destroyed.
    crate::routes_task_ask::close_ask_session(state, id).await;

    // Capture old PR info before rework clears it. We close the PR on GitHub
    // only AFTER rework_item succeeds, so a validation failure (e.g. task is
    // in CaptainReviewing) doesn't leave an irreversibly closed PR.
    let old_pr_info: Option<(String, String)> = {
        let store = state.task_store.read().await;
        match store.find_by_id(id).await {
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
        }
    };

    let store = state.task_store.write().await;
    mando_captain::runtime::dashboard::rework_item(&store, id, &body.feedback)
        .await
        .map_err(|e| internal_error(e, "failed to rework task"))?;

    // Best-effort image persistence -- rework is already committed so
    // a failure here should not return an error to the client.
    if !body.saved_images.is_empty() {
        if let Err(e) =
            crate::image_upload::append_task_images(&store, id, &body.saved_images).await
        {
            tracing::warn!(task_id = id, error = ?e, "failed to persist rework images");
        }
    }

    let summary = if body.feedback.is_empty() {
        "Rework requested".to_string()
    } else {
        format!("Rework requested: {}", body.feedback)
    };
    if let Some(item) = store
        .find_by_id(id)
        .await
        .map_err(|e| internal_error(e, "failed to load task"))?
    {
        let _ = mando_captain::runtime::timeline_emit::emit_for_task(
            &item,
            mando_types::timeline::TimelineEventType::ReworkRequested,
            &summary,
            json!({"content": &body.feedback}),
            store.pool(),
        )
        .await;
    }
    // Drop the write lock before the external GitHub CLI call.
    drop(store);

    // Close the old PR on GitHub after rework succeeds.
    if let Some((pr_num, repo)) = old_pr_info {
        if let Err(e) = mando_captain::io::github::close_pr(&repo, &pr_num).await {
            tracing::warn!(
                module = "gateway",
                task_id = id,
                pr = %pr_num,
                error = %e,
                "failed to close old PR during rework — continuing anyway"
            );
        }
    }
    let updated = {
        let store = state.task_store.read().await;
        store
            .find_by_id(id)
            .await
            .ok()
            .flatten()
            .map(|t| serde_json::to_value(&t).unwrap())
    };
    let wb_id = updated
        .as_ref()
        .and_then(|v| v.get("workbench_id"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    state.bus.send(
        mando_types::BusEvent::Tasks,
        Some(json!({"action": "updated", "item": updated, "id": id})),
    );
    touch_workbench_activity(state, wb_id).await;
    Ok(Json(json!({"ok": true})))
}

/// POST /api/tasks/retry — re-trigger CaptainReviewing for Errored items.
pub(crate) async fn post_task_retry(
    State(state): State<AppState>,
    Json(body): Json<IdBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let id = body.id;
    let store = state.task_store.read().await;
    mando_captain::runtime::dashboard::retry_item(&store, id)
        .await
        .map_err(|e| internal_error(e, "failed to retry task"))?;
    if let Some(item) = store
        .find_by_id(id)
        .await
        .map_err(|e| internal_error(e, "failed to load task"))?
    {
        let _ = mando_captain::runtime::timeline_emit::emit_for_task(
            &item,
            mando_types::timeline::TimelineEventType::StatusChanged,
            "Retried — re-entering captain review",
            json!({"from": "errored", "to": "captain-reviewing"}),
            store.pool(),
        )
        .await;
    }
    let updated = store
        .find_by_id(id)
        .await
        .ok()
        .flatten()
        .map(|t| serde_json::to_value(&t).unwrap());
    let wb_id = updated
        .as_ref()
        .and_then(|v| v.get("workbench_id"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    state.bus.send(
        mando_types::BusEvent::Tasks,
        Some(json!({"action": "updated", "item": updated, "id": id})),
    );
    touch_workbench_activity(&state, wb_id).await;
    Ok(Json(json!({"ok": true})))
}

/// POST /api/tasks/resume-rate-limited — clear global rate-limit cooldown and
/// trigger a captain tick so that the identified task (and any others blocked
/// by the cooldown) are picked up immediately.
pub(crate) async fn post_task_resume_rate_limited(
    State(state): State<AppState>,
    Json(body): Json<IdBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let id = body.id;
    {
        let store = state.task_store.read().await;
        mando_captain::runtime::dashboard::validate_rate_limited_task(&store, id)
            .await
            .map_err(|e| internal_error(e, "failed to validate rate-limited task"))?;
        if let Some(item) = store
            .find_by_id(id)
            .await
            .map_err(|e| internal_error(e, "failed to load task"))?
        {
            let _ = mando_captain::runtime::timeline_emit::emit_for_task(
                &item,
                mando_types::timeline::TimelineEventType::RateLimited,
                "Rate-limit cooldown cleared manually — resuming",
                json!({"action": "resume-rate-limited", "cleared_by": "human"}),
                store.pool(),
            )
            .await;
        }
        let updated = store
            .find_by_id(id)
            .await
            .ok()
            .flatten()
            .map(|t| serde_json::to_value(&t).unwrap());
        let wb_id = updated
            .as_ref()
            .and_then(|v| v.get("workbench_id"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        state.bus.send(
            mando_types::BusEvent::Tasks,
            Some(json!({"action": "updated", "item": updated, "id": id})),
        );
        touch_workbench_activity(&state, wb_id).await;
    }
    // Trigger a captain tick so the task resumes immediately.
    let config = state.config.load_full();
    let workflow = state.captain_workflow.load_full();
    mando_captain::runtime::dashboard::trigger_captain_tick(
        &config,
        &workflow,
        false,
        Some(&state.bus),
        false,
        &state.task_store,
        &state.cancellation_token,
    )
    .await
    .map_err(|e| internal_error(e, "failed to trigger captain tick"))?;
    Ok(Json(json!({"ok": true})))
}

/// POST /api/tasks/handoff
pub(crate) async fn post_task_handoff(
    State(state): State<AppState>,
    Json(body): Json<IdBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let id = body.id;
    let store = state.task_store.read().await;
    let pool = state.db.pool();
    simple_task_action(
        &state,
        id,
        mando_captain::runtime::dashboard::handoff_item(&store, id, pool),
    )
    .await
}
