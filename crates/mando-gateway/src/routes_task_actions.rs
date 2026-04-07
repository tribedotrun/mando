//! Task lifecycle-action route handlers (accept, cancel, reopen, rework, handoff).

use std::future::Future;

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::response::{error_response, internal_error};
use crate::AppState;

#[derive(Deserialize)]
pub(crate) struct IdBody {
    pub id: i64,
}

#[derive(Deserialize)]
pub(crate) struct FeedbackBody {
    pub id: i64,
    #[serde(default)]
    pub feedback: String,
}

/// Shared wrapper for simple task actions that take a task store and an id,
/// return `anyhow::Result<()>`, and emit a `Tasks` bus event on success.
async fn simple_task_action<Fut>(
    state: &AppState,
    id: i64,
    action: &'static str,
    work: Fut,
) -> Result<Json<Value>, (StatusCode, Json<Value>)>
where
    Fut: Future<Output = anyhow::Result<()>>,
{
    work.await.map_err(internal_error)?;
    state.bus.send(
        mando_types::BusEvent::Tasks,
        Some(json!({"action": action, "id": id})),
    );
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
        "accept",
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
        "cancel",
        mando_captain::runtime::dashboard::cancel_item(&store, id, pool),
    )
    .await
}

/// POST /api/tasks/reopen
pub(crate) async fn post_task_reopen(
    State(state): State<AppState>,
    Json(body): Json<FeedbackBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let id = body.id;
    let config = state.config.load_full();
    let workflow = state.captain_workflow.load_full();
    let notifier = crate::captain_notifier(&state, &config);
    let store = state.task_store.write().await;
    let mut item = store
        .find_by_id(id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "item not found"))?;
    // Close any active ask session — the worker will modify the codebase.
    crate::routes_task_ask::close_ask_session(&state, id).await;

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
    .map_err(internal_error)?;
    store.write_task(&item).await.map_err(internal_error)?;

    state.bus.send(
        mando_types::BusEvent::Tasks,
        Some(json!({"action": "reopen"})),
    );

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

/// POST /api/tasks/rework
pub(crate) async fn post_task_rework(
    State(state): State<AppState>,
    Json(body): Json<FeedbackBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let id = body.id;

    // Close any active ask session — the worktree will be destroyed.
    crate::routes_task_ask::close_ask_session(&state, id).await;

    // Capture old PR info before rework clears it. We close the PR on GitHub
    // only AFTER rework_item succeeds, so a validation failure (e.g. task is
    // in CaptainReviewing) doesn't leave an irreversibly closed PR.
    let old_pr_info: Option<(String, String)> = {
        let store = state.task_store.read().await;
        match store.find_by_id(id).await {
            Ok(Some(item)) => {
                let pr_num = item
                    .pr
                    .as_deref()
                    .and_then(mando_types::task::extract_pr_number)
                    .map(|s| s.to_string());
                let config = state.config.load_full();
                let repo = mando_config::resolve_github_repo(item.project.as_deref(), &config);
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
        .map_err(internal_error)?;

    let summary = if body.feedback.is_empty() {
        "Rework requested".to_string()
    } else {
        format!("Rework requested: {}", body.feedback)
    };
    if let Some(item) = store.find_by_id(id).await.map_err(internal_error)? {
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
    state.bus.send(
        mando_types::BusEvent::Tasks,
        Some(json!({"action": "rework"})),
    );
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
        .map_err(internal_error)?;
    if let Some(item) = store.find_by_id(id).await.map_err(internal_error)? {
        let _ = mando_captain::runtime::timeline_emit::emit_for_task(
            &item,
            mando_types::timeline::TimelineEventType::StatusChanged,
            "Retried — re-entering captain review",
            json!({"from": "errored", "to": "captain-reviewing"}),
            store.pool(),
        )
        .await;
    }
    state.bus.send(
        mando_types::BusEvent::Tasks,
        Some(json!({"action": "retry"})),
    );
    Ok(Json(json!({"ok": true})))
}

/// Shared logic for archive/unarchive: call a DB function returning `Result<bool>`,
/// emit a bus event on success, and map the result to JSON.
async fn archive_toggle(
    state: &AppState,
    id: i64,
    action: &str,
    db_fn: impl std::future::Future<Output = anyhow::Result<bool>>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    match db_fn.await {
        Ok(true) => {
            state.bus.send(
                mando_types::BusEvent::Tasks,
                Some(json!({"action": action, "id": id})),
            );
            Ok(Json(json!({"ok": true})))
        }
        Ok(false) => Err(error_response(
            StatusCode::NOT_FOUND,
            &format!("item {id} not found"),
        )),
        Err(e) => Err(error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &e.to_string(),
        )),
    }
}

/// POST /api/tasks/{id}/archive
pub(crate) async fn post_task_archive(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<i64>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let pool = state.task_store.write().await.pool().clone();
    archive_toggle(
        &state,
        id,
        "archive",
        mando_db::queries::tasks::archive_by_id(&pool, id),
    )
    .await
}

/// POST /api/tasks/{id}/unarchive
pub(crate) async fn post_task_unarchive(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<i64>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let pool = state.task_store.write().await.pool().clone();
    archive_toggle(
        &state,
        id,
        "unarchive",
        mando_db::queries::tasks::unarchive(&pool, id),
    )
    .await
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
        "handoff",
        mando_captain::runtime::dashboard::handoff_item(&store, id, pool),
    )
    .await
}
