//! Config management endpoints for the daemon.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde_json::json;

use crate::AppState;

/// GET /api/config — read current config.
#[crate::instrument_api(method = "GET", path = "/api/config")]
pub(crate) async fn get_config(
    State(state): State<AppState>,
) -> Result<Json<api_types::MandoConfig>, StatusCode> {
    let config = state.settings.load_config();
    let mut val = serde_json::to_value(&*config).map_err(|e| {
        tracing::error!(module = "transport-http-transport-routes_config", error = %e, "failed to serialize config");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    crate::runtime::config_support::inject_projects(&config, &mut val);
    let config = serde_json::from_value(val).map_err(|e| {
        tracing::error!(module = "transport-http-transport-routes_config", error = %e, "failed to convert config to api-types");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(config))
}

/// PUT /api/config — write config.json, hot-reload into daemon.
#[crate::instrument_api(method = "PUT", path = "/api/config")]
pub(crate) async fn put_config(
    State(state): State<AppState>,
    Json(body): Json<api_types::MandoConfig>,
) -> Result<Json<api_types::ConfigWriteResponse>, axum::response::Response> {
    let new_config: settings::config::Config =
        serde_json::from_value(serde_json::to_value(body).map_err(|err| {
            (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": format!("invalid config: {err}")})),
            )
                .into_response()
        })?)
        .map_err(|err| {
            (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": format!("invalid config: {err}")})),
            )
                .into_response()
        })?;

    let outcome = match state.settings.apply_api_config(new_config).await {
        Ok(outcome) => outcome,
        Err(err) => return Err(config_error_response(err)),
    };

    let committed_config = state.settings.load_config();
    apply_config_outcome(&state, &committed_config, outcome).await;

    let configured_paths = captain::resolve_captain_runtime_paths(&committed_config);
    Ok(Json(api_types::ConfigWriteResponse {
        ok: true,
        restart_required: state.runtime_paths != configured_paths,
        task_db_path: state
            .runtime_paths
            .task_db_path
            .to_string_lossy()
            .into_owned(),
        worker_health_path: state
            .runtime_paths
            .worker_health_path
            .to_string_lossy()
            .into_owned(),
        lockfile_path: state
            .runtime_paths
            .lockfile_path
            .to_string_lossy()
            .into_owned(),
    }))
}

/// GET /api/config/status — returns whether config exists and setup is complete.
#[crate::instrument_api(method = "GET", path = "/api/config/status")]
pub(crate) async fn get_config_status(
    State(state): State<AppState>,
) -> Json<api_types::ConfigStatusResponse> {
    let config_path = settings::config::get_config_path();
    let exists = config_path.exists();
    let config = state.settings.load_config();
    let active_paths = state.runtime_paths.clone();
    let configured_paths = captain::resolve_captain_runtime_paths(&config);
    let (setup_complete, error) = if exists {
        match tokio::fs::read_to_string(&config_path).await {
            Ok(contents) => match serde_json::from_str::<settings::config::Config>(&contents) {
                Ok(_) => (true, None),
                Err(err) => {
                    tracing::warn!(module = "transport-http-transport-routes_config", path = %config_path.display(), error = %err, "config.json exists but is corrupt");
                    (false, Some(format!("corrupt config: {err}")))
                }
            },
            Err(err) => {
                tracing::warn!(module = "transport-http-transport-routes_config", path = %config_path.display(), error = %err, "config.json exists but is unreadable");
                (false, Some(format!("unreadable: {err}")))
            }
        }
    } else {
        (false, None)
    };
    Json(api_types::ConfigStatusResponse {
        exists,
        setup_complete,
        error,
        task_db_path: active_paths.task_db_path.to_string_lossy().into_owned(),
        worker_health_path: active_paths
            .worker_health_path
            .to_string_lossy()
            .into_owned(),
        lockfile_path: active_paths.lockfile_path.to_string_lossy().into_owned(),
        configured_task_db_path: configured_paths.task_db_path.to_string_lossy().into_owned(),
        configured_worker_health_path: configured_paths
            .worker_health_path
            .to_string_lossy()
            .into_owned(),
        configured_lockfile_path: configured_paths
            .lockfile_path
            .to_string_lossy()
            .into_owned(),
        restart_required: active_paths != configured_paths,
    })
}

/// POST /api/config/setup — mark first-launch setup complete.
#[crate::instrument_api(method = "POST", path = "/api/config/setup")]
pub(crate) async fn post_config_setup(
    State(state): State<AppState>,
    Json(body): Json<api_types::ConfigSetupRequest>,
) -> Result<Json<api_types::ConfigSetupResponse>, axum::response::Response> {
    if let Some(config_body) = body.config {
        let new_config: settings::config::Config =
            serde_json::from_value(serde_json::to_value(config_body).map_err(|err| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error": format!("invalid config: {err}")})),
                )
                    .into_response()
            })?)
            .map_err(|err| {
                (
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error": format!("invalid config: {err}")})),
                )
                    .into_response()
            })?;

        let outcome = match state.settings.apply_api_config(new_config).await {
            Ok(outcome) => outcome,
            Err(err) => return Err(config_error_response(err)),
        };

        let committed_config = state.settings.load_config();
        apply_config_outcome(&state, &committed_config, outcome).await;
    }

    Ok(Json(api_types::ConfigSetupResponse { ok: true }))
}

/// GET /api/config/paths — returns config/runtime paths.
#[crate::instrument_api(method = "GET", path = "/api/config/paths")]
pub(crate) async fn get_config_paths(
    State(state): State<AppState>,
) -> Json<api_types::ConfigPathsResponse> {
    let config = state.settings.load_config();
    let active_paths = state.runtime_paths.clone();
    let configured_paths = captain::resolve_captain_runtime_paths(&config);
    Json(api_types::ConfigPathsResponse {
        data_dir: global_infra::paths::data_dir()
            .to_string_lossy()
            .into_owned(),
        config_path: settings::config::get_config_path()
            .to_string_lossy()
            .into_owned(),
        task_db_path: active_paths.task_db_path.to_string_lossy().into_owned(),
        worker_health_path: active_paths
            .worker_health_path
            .to_string_lossy()
            .into_owned(),
        lockfile_path: active_paths.lockfile_path.to_string_lossy().into_owned(),
        configured_task_db_path: configured_paths.task_db_path.to_string_lossy().into_owned(),
        configured_worker_health_path: configured_paths
            .worker_health_path
            .to_string_lossy()
            .into_owned(),
        configured_lockfile_path: configured_paths
            .lockfile_path
            .to_string_lossy()
            .into_owned(),
        restart_required: active_paths != configured_paths,
    })
}

async fn apply_config_outcome(
    state: &AppState,
    committed_config: &settings::config::Config,
    outcome: settings::ConfigApplyOutcome,
) {
    if outcome.reload_telegram {
        if let Err(err) = state.telegram_runtime.configure(committed_config).await {
            tracing::warn!(module = "telegram", error = %err, "telegram hot reload failed");
        }
    }

    if outcome.publish_config_event {
        state.bus.send(global_bus::BusPayload::Config(None));
    }
    if outcome.publish_status_event {
        state.bus.send(global_bus::BusPayload::Status(None));
    }
}

fn config_error_response(err: settings::ApplyConfigError) -> axum::response::Response {
    match err {
        settings::ApplyConfigError::Validation(message) => {
            (StatusCode::BAD_REQUEST, Json(json!({"error": message}))).into_response()
        }
        settings::ApplyConfigError::WorkflowReload(message) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": message})),
        )
            .into_response(),
        settings::ApplyConfigError::Internal(err) => {
            crate::response::internal_error(err, "save failed").into_response()
        }
    }
}
