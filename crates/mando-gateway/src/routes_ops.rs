//! /api/ops/* route handlers — multi-turn CC sessions (ask, etc.).

use std::time::Duration;

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::response::error_response;
use crate::AppState;

/// Resolve the working directory from config (first project path). Returns
/// 400 if no project is configured — the previous behaviour silently
/// defaulted to an empty PathBuf which blew up downstream.
fn resolve_cwd(state: &AppState) -> Result<std::path::PathBuf, (StatusCode, Json<Value>)> {
    let cfg = state.config.load_full();
    mando_config::paths::first_project_path(&cfg)
        .map(|p| mando_config::paths::expand_tilde(&p))
        .ok_or_else(|| {
            error_response(
                StatusCode::BAD_REQUEST,
                "no project configured — cannot run ops session",
            )
        })
}

#[derive(Deserialize)]
pub(crate) struct OpsStartBody {
    pub key: String,
    pub prompt: String,
    #[serde(default)]
    pub model: Option<String>,
}

/// POST /api/ops/start — start a new ops CC session.
pub(crate) async fn post_ops_start(
    State(state): State<AppState>,
    Json(body): Json<OpsStartBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let cwd = resolve_cwd(&state)?;

    let prompt = body.prompt;

    // `start_replacing` atomically closes any existing session for this key
    // and starts a fresh one under the per-key async mutex, so two concurrent
    // `/api/ops/start` calls with the same key cannot clobber each other.
    match state
        .cc_session_mgr
        .start_replacing(
            &body.key,
            &prompt,
            &cwd,
            body.model.as_deref(),
            Duration::from_secs(3600),
            Duration::from_secs(120),
        )
        .await
    {
        Ok(result) => Ok(Json(json!({
            "ok": true,
            "result_text": result.text,
            "session_id": result.session_id,
            "cost_usd": result.cost_usd,
            "duration_ms": result.duration_ms,
        }))),
        Err(e) => Err(crate::response::internal_error(e)),
    }
}

#[derive(Deserialize)]
pub(crate) struct OpsMessageBody {
    pub key: String,
    pub message: String,
}

/// POST /api/ops/message — send a follow-up message to an active ops session.
pub(crate) async fn post_ops_message(
    State(state): State<AppState>,
    Json(body): Json<OpsMessageBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let cwd = resolve_cwd(&state)?;
    let mgr = &state.cc_session_mgr;

    if !mgr.has_session(&body.key) {
        return Err(error_response(
            StatusCode::NOT_FOUND,
            &format!("no active ops session for '{}'", body.key),
        ));
    }

    match mgr.follow_up(&body.key, &body.message, &cwd).await {
        Ok(result) => Ok(Json(json!({
            "ok": true,
            "result_text": result.text,
            "session_id": result.session_id,
            "cost_usd": result.cost_usd,
            "duration_ms": result.duration_ms,
        }))),
        Err(e) => Err(crate::response::internal_error(e)),
    }
}

#[derive(Deserialize)]
pub(crate) struct OpsEndBody {
    pub key: String,
}

/// POST /api/ops/end — end an active ops session.
pub(crate) async fn post_ops_end(
    State(state): State<AppState>,
    Json(body): Json<OpsEndBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if !state.cc_session_mgr.has_session(&body.key) {
        return Err(error_response(
            StatusCode::NOT_FOUND,
            &format!("no active ops session for '{}'", body.key),
        ));
    }

    // `close_async` acquires the per-key mutex so it cannot race with a
    // concurrent start/follow_up on the same key.
    state.cc_session_mgr.close_async(&body.key).await;
    Ok(Json(json!({"ok": true, "ended": body.key})))
}
