use std::collections::HashMap;

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::response::{error_response, internal_error};
use crate::ui_runtime::UiLaunchSpec;
use crate::AppState;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UiRegisterBody {
    pub pid: i32,
    pub exec_path: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub cwd: Option<String>,
    #[serde(default)]
    pub env: HashMap<String, String>,
}

pub(crate) async fn post_ui_register(
    State(state): State<AppState>,
    Json(body): Json<UiRegisterBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    state
        .ui_runtime
        .register(
            body.pid,
            UiLaunchSpec {
                exec_path: body.exec_path,
                args: body.args,
                cwd: body.cwd,
                env: body.env,
            },
        )
        .await
        .map_err(|err| internal_error(err, "failed to register UI process"))?;

    Ok(Json(json!({ "ok": true })))
}

pub(crate) async fn post_ui_quitting(
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    state
        .ui_runtime
        .mark_quitting()
        .await
        .map_err(|err| internal_error(err, "failed to mark UI quitting"))?;
    Ok(Json(json!({ "ok": true })))
}

pub(crate) async fn post_ui_updating(
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    state
        .ui_runtime
        .mark_updating()
        .await
        .map_err(|err| internal_error(err, "failed to mark UI updating"))?;
    Ok(Json(json!({ "ok": true })))
}

pub(crate) async fn post_ui_launch(
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    state
        .ui_runtime
        .launch()
        .await
        .map_err(|err| error_response(StatusCode::BAD_REQUEST, &err.to_string()))?;
    Ok(Json(json!({ "ok": true })))
}

pub(crate) async fn post_ui_restart(
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    state
        .ui_runtime
        .restart()
        .await
        .map_err(|err| error_response(StatusCode::BAD_REQUEST, &err.to_string()))?;
    Ok(Json(json!({ "ok": true })))
}

pub(crate) async fn get_ui_health(State(state): State<AppState>) -> Json<Value> {
    let status = state.ui_runtime.status().await;
    match serde_json::to_value(&status) {
        Ok(v) => Json(v),
        Err(e) => {
            tracing::error!(module = "ui", error = %e, "failed to serialize UiStatus");
            Json(json!({ "healthy": false, "error": e.to_string() }))
        }
    }
}

pub(crate) async fn get_telegram_health(State(state): State<AppState>) -> Json<Value> {
    let status = state.telegram_runtime.status().await;
    match serde_json::to_value(&status) {
        Ok(v) => Json(v),
        Err(e) => {
            tracing::error!(module = "telegram", error = %e, "failed to serialize TelegramStatus");
            Json(json!({ "healthy": false, "error": e.to_string() }))
        }
    }
}
