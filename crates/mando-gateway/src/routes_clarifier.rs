//! /api/clarifier/* route handlers — multi-turn clarifier sessions via HTTP.

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::response::error_response;
use crate::AppState;

fn clarifier_result_json(r: &mando_captain::runtime::clarifier::ClarifierResult) -> Value {
    json!({
        "status": format!("{:?}", r.status),
        "context": r.context,
        "questions": r.questions,
        "generated_title": r.generated_title,
        "repo": r.repo,
        "no_pr": r.no_pr,
        "resource": r.resource,
    })
}

#[derive(Deserialize)]
pub(crate) struct ClarifierStartBody {
    pub key: String,
    pub item_id: String,
    pub message: String,
}

/// POST /api/clarifier/start — start a new multi-turn clarifier session.
pub(crate) async fn post_clarifier_start(
    State(state): State<AppState>,
    Json(body): Json<ClarifierStartBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    // Load the task item by ID from the TaskStore.
    let store = state.task_store.read().await;
    let item = store
        .find_by_id(body.item_id.parse::<i64>().map_err(|_| {
            error_response(
                StatusCode::BAD_REQUEST,
                &format!("invalid id: {}", body.item_id),
            )
        })?)
        .await
        .map_err(|e| error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?
        .ok_or_else(|| {
            error_response(
                StatusCode::NOT_FOUND,
                &format!("task '{}' not found", body.item_id),
            )
        })?;
    drop(store);

    let mut mgr = state.clarifier_mgr.write().await;
    let wf = state.captain_workflow.load_full();
    let cfg = state.config.load_full();
    match mgr.start(&body.key, &item, &body.message, &wf, &cfg).await {
        Ok(result) => Ok(Json(json!({
            "ok": true,
            "result": clarifier_result_json(&result),
        }))),
        Err(e) => Err(error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &e.to_string(),
        )),
    }
}

#[derive(Deserialize)]
pub(crate) struct ClarifierMessageBody {
    pub key: String,
    pub message: String,
}

/// POST /api/clarifier/message — send a follow-up message to an active session.
pub(crate) async fn post_clarifier_message(
    State(state): State<AppState>,
    Json(body): Json<ClarifierMessageBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let mut mgr = state.clarifier_mgr.write().await;

    if !mgr.has_session(&body.key) {
        return Err(error_response(
            StatusCode::NOT_FOUND,
            &format!("no active clarifier session for '{}'", body.key),
        ));
    }

    match mgr.follow_up(&body.key, &body.message).await {
        Ok(result) => Ok(Json(json!({
            "ok": true,
            "result": clarifier_result_json(&result),
        }))),
        Err(e) => Err(error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &e.to_string(),
        )),
    }
}

#[derive(Deserialize)]
pub(crate) struct ClarifierCancelBody {
    pub key: String,
}

/// POST /api/clarifier/cancel — cancel an active clarifier session.
pub(crate) async fn post_clarifier_cancel(
    State(state): State<AppState>,
    Json(body): Json<ClarifierCancelBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let mut mgr = state.clarifier_mgr.write().await;

    if !mgr.has_session(&body.key) {
        return Err(error_response(
            StatusCode::NOT_FOUND,
            &format!("no active clarifier session for '{}'", body.key),
        ));
    }

    mgr.close(&body.key);
    Ok(Json(json!({"ok": true, "cancelled": body.key})))
}
