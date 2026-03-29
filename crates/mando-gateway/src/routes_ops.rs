//! /api/ops/* route handlers — multi-turn CC ops/ask sessions via HTTP.

use std::time::Duration;

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::response::error_response;
use crate::AppState;

/// Resolve the working directory from config (first project path).
async fn resolve_cwd(state: &AppState) -> std::path::PathBuf {
    let cfg = state.config.load_full();
    mando_config::paths::first_project_path(&cfg)
        .map(|p| mando_config::paths::expand_tilde(&p))
        .unwrap_or_default()
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
    if !state.config.load().features.dev_mode {
        return Err(error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "dev mode is disabled",
        ));
    }

    let cwd = resolve_cwd(&state).await;

    let prompt = format!(
        "You are an ops copilot for a software development team. \
         Help with the following request. Be concise.\n\n{}",
        body.prompt
    );

    let mut mgr = state.cc_session_mgr.write().await;

    // Close any existing session for this key before starting fresh.
    if mgr.has_session(&body.key) {
        mgr.close(&body.key);
    }

    match mgr
        .start(
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
        Err(e) => Err(error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &e.to_string(),
        )),
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
    if !state.config.load().features.dev_mode {
        return Err(error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "dev mode is disabled",
        ));
    }

    let cwd = resolve_cwd(&state).await;
    let mut mgr = state.cc_session_mgr.write().await;

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
        Err(e) => Err(error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &e.to_string(),
        )),
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
    if !state.config.load().features.dev_mode {
        return Err(error_response(
            StatusCode::SERVICE_UNAVAILABLE,
            "dev mode is disabled",
        ));
    }

    let mut mgr = state.cc_session_mgr.write().await;

    if !mgr.has_session(&body.key) {
        return Err(error_response(
            StatusCode::NOT_FOUND,
            &format!("no active ops session for '{}'", body.key),
        ));
    }

    mgr.close(&body.key);
    Ok(Json(json!({"ok": true, "ended": body.key})))
}
