use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;

use crate::response::{error_response, internal_error, ApiError};
use crate::AppState;
use transport_ui::UiLaunchSpec;

#[crate::instrument_api(method = "POST", path = "/api/ui/register")]
pub(crate) async fn post_ui_register(
    State(state): State<AppState>,
    Json(body): Json<api_types::UiRegisterRequest>,
) -> Result<Json<api_types::BoolOkResponse>, ApiError> {
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

    Ok(Json(api_types::BoolOkResponse { ok: true }))
}

#[crate::instrument_api(method = "POST", path = "/api/ui/quitting")]
pub(crate) async fn post_ui_quitting(
    State(state): State<AppState>,
    Json(_body): Json<api_types::EmptyRequest>,
) -> Result<Json<api_types::BoolOkResponse>, ApiError> {
    state
        .ui_runtime
        .mark_quitting()
        .await
        .map_err(|err| internal_error(err, "failed to mark UI quitting"))?;
    Ok(Json(api_types::BoolOkResponse { ok: true }))
}

#[crate::instrument_api(method = "POST", path = "/api/ui/updating")]
pub(crate) async fn post_ui_updating(
    State(state): State<AppState>,
    Json(_body): Json<api_types::EmptyRequest>,
) -> Result<Json<api_types::BoolOkResponse>, ApiError> {
    state
        .ui_runtime
        .mark_updating()
        .await
        .map_err(|err| internal_error(err, "failed to mark UI updating"))?;
    Ok(Json(api_types::BoolOkResponse { ok: true }))
}

#[crate::instrument_api(method = "POST", path = "/api/ui/launch")]
pub(crate) async fn post_ui_launch(
    State(state): State<AppState>,
    Json(_body): Json<api_types::EmptyRequest>,
) -> Result<Json<api_types::BoolOkResponse>, ApiError> {
    state
        .ui_runtime
        .launch()
        .await
        .map_err(|err| error_response(StatusCode::BAD_REQUEST, &err.to_string()))?;
    Ok(Json(api_types::BoolOkResponse { ok: true }))
}

#[crate::instrument_api(method = "POST", path = "/api/ui/restart")]
pub(crate) async fn post_ui_restart(
    State(state): State<AppState>,
    Json(_body): Json<api_types::EmptyRequest>,
) -> Result<Json<api_types::BoolOkResponse>, ApiError> {
    state
        .ui_runtime
        .restart()
        .await
        .map_err(|err| error_response(StatusCode::BAD_REQUEST, &err.to_string()))?;
    Ok(Json(api_types::BoolOkResponse { ok: true }))
}

pub(crate) async fn get_telegram_health(
    State(state): State<AppState>,
) -> Json<api_types::TelegramHealth> {
    let status = state.telegram_runtime.status().await;
    Json(api_types::TelegramHealth {
        enabled: status.enabled,
        running: status.running,
        owner: status.owner,
        last_error: status.last_error,
        degraded: status.degraded,
        restart_count: u64::from(status.restart_count),
        mode: status.mode.to_string(),
    })
}
