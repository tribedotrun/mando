//! Task lifecycle-action route handlers (accept, cancel, reopen, rework, ask, handoff).

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

#[derive(Deserialize)]
pub(crate) struct AskBody {
    pub id: i64,
    pub question: String,
}

/// POST /api/tasks/accept
pub(crate) async fn post_task_accept(
    State(state): State<AppState>,
    Json(body): Json<IdBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let id = body.id;
    let store = state.task_store.read().await;
    match mando_captain::runtime::dashboard::accept_item(&store, id).await {
        Ok(()) => {
            state.bus.send(
                mando_types::BusEvent::Tasks,
                Some(json!({"action": "accept"})),
            );
            Ok(Json(json!({"ok": true})))
        }
        Err(e) => Err(error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &e.to_string(),
        )),
    }
}

/// POST /api/tasks/cancel
pub(crate) async fn post_task_cancel(
    State(state): State<AppState>,
    Json(body): Json<IdBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let id = body.id;
    let store = state.task_store.read().await;
    let pool = state.db.pool();
    match mando_captain::runtime::dashboard::cancel_item(&store, id, pool).await {
        Ok(()) => {
            state.bus.send(
                mando_types::BusEvent::Tasks,
                Some(json!({"action": "cancel"})),
            );
            Ok(Json(json!({"ok": true})))
        }
        Err(e) => Err(error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &e.to_string(),
        )),
    }
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
    let store = state.task_store.read().await;
    let mut item = store
        .find_by_id(id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "item not found"))?;
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
    mando_captain::runtime::timeline_emit::emit_for_task(
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
    let store = state.task_store.read().await;
    match mando_captain::runtime::dashboard::rework_item(&store, id, &body.feedback).await {
        Ok(()) => {
            let summary = if body.feedback.is_empty() {
                "Rework requested".to_string()
            } else {
                format!("Rework requested: {}", body.feedback)
            };
            if let Some(item) = store.find_by_id(id).await.unwrap_or(None) {
                mando_captain::runtime::timeline_emit::emit_for_task(
                    &item,
                    mando_types::timeline::TimelineEventType::ReworkRequested,
                    &summary,
                    json!({"content": &body.feedback}),
                    store.pool(),
                )
                .await;
            }
            state.bus.send(
                mando_types::BusEvent::Tasks,
                Some(json!({"action": "rework"})),
            );
            Ok(Json(json!({"ok": true})))
        }
        Err(e) => Err(error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &e.to_string(),
        )),
    }
}

/// POST /api/tasks/ask
pub(crate) async fn post_task_ask(
    State(state): State<AppState>,
    Json(body): Json<AskBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let id = body.id;
    let config = state.config.load_full();
    let (item, pool) = {
        let store = state.task_store.read().await;
        let item = store.find_by_id(id).await.unwrap_or(None);
        let pool = store.pool().clone();
        (item, pool)
    };
    let item = match item {
        Some(it) => it,
        None => {
            return Err(error_response(
                StatusCode::NOT_FOUND,
                &format!("item {} not found", body.id),
            ))
        }
    };
    let workflow = state.captain_workflow.load_full();
    match mando_captain::runtime::task_ask::ask_task_with(
        &config,
        &item,
        id,
        &pool,
        &body.question,
        &workflow,
    )
    .await
    {
        Ok(val) => {
            state.bus.send(
                mando_types::BusEvent::Tasks,
                Some(json!({"action": "ask", "id": id})),
            );
            Ok(Json(val))
        }
        Err(e) => Err(error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &e.to_string(),
        )),
    }
}

/// POST /api/tasks/retry — re-trigger CaptainReviewing for Errored items.
pub(crate) async fn post_task_retry(
    State(state): State<AppState>,
    Json(body): Json<IdBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let id = body.id;
    let store = state.task_store.read().await;
    match mando_captain::runtime::dashboard::retry_item(&store, id).await {
        Ok(()) => {
            if let Some(item) = store.find_by_id(id).await.unwrap_or(None) {
                mando_captain::runtime::timeline_emit::emit_for_task(
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
    let store = state.task_store.read().await;
    let pool = store.pool();
    match mando_db::queries::tasks::archive_by_id(pool, id).await {
        Ok(true) => {
            state.bus.send(
                mando_types::BusEvent::Tasks,
                Some(json!({"action": "archive", "id": id})),
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

/// POST /api/tasks/{id}/unarchive
pub(crate) async fn post_task_unarchive(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<i64>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let store = state.task_store.read().await;
    let pool = store.pool();
    match mando_db::queries::tasks::unarchive(pool, id).await {
        Ok(true) => {
            state.bus.send(
                mando_types::BusEvent::Tasks,
                Some(json!({"action": "unarchive", "id": id})),
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

/// POST /api/tasks/handoff
pub(crate) async fn post_task_handoff(
    State(state): State<AppState>,
    Json(body): Json<IdBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let id = body.id;
    let store = state.task_store.read().await;
    let pool = state.db.pool();
    match mando_captain::runtime::dashboard::handoff_item(&store, id, pool).await {
        Ok(()) => {
            state.bus.send(
                mando_types::BusEvent::Tasks,
                Some(json!({"action": "handoff"})),
            );
            Ok(Json(json!({"ok": true})))
        }
        Err(e) => Err(error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &e.to_string(),
        )),
    }
}
