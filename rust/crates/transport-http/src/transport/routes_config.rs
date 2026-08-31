//! Config management endpoints for the daemon.

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde_json::json;

use crate::AppState;

/// GET /api/config — read current config.
pub(crate) async fn get_config(
    State(state): State<AppState>,
) -> Result<Json<api_types::MandoConfig>, StatusCode> {
    let config = state.settings.load_config();
    let api_config = crate::runtime::config_support::config_to_api(&config).map_err(|e| {
        tracing::error!(module = "transport-http-transport-routes_config", error = %e, "failed to convert config to api-types");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(api_config))
}

/// PUT /api/config — write config.json, hot-reload into daemon.
pub(crate) async fn put_config(
    State(state): State<AppState>,
    Json(body): Json<api_types::MandoConfig>,
) -> Result<Json<api_types::ConfigWriteResponse>, axum::response::Response> {
    let new_config = crate::runtime::config_support::config_from_api(body);

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
pub(crate) async fn get_config_status(
    State(state): State<AppState>,
) -> Json<api_types::ConfigStatusResponse> {
    let config_path = settings::get_config_path();
    let exists = config_path.exists();
    let config = state.settings.load_config();
    let active_paths = state.runtime_paths.clone();
    let configured_paths = captain::resolve_captain_runtime_paths(&config);
    let (setup_complete, error) = if exists {
        match tokio::fs::read_to_string(&config_path).await {
            Ok(contents) => match parse_config_status_contents(&contents, &config_path) {
                Ok(()) => (true, None),
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
pub(crate) async fn post_config_setup(
    State(state): State<AppState>,
    Json(body): Json<api_types::ConfigSetupRequest>,
) -> Result<Json<api_types::ConfigSetupResponse>, axum::response::Response> {
    if let Some(config_body) = body.config {
        let new_config = crate::runtime::config_support::config_from_api(config_body);

        let outcome = match state.settings.apply_api_config(new_config).await {
            Ok(outcome) => outcome,
            Err(err) => return Err(config_error_response(err)),
        };

        let committed_config = state.settings.load_config();
        apply_config_outcome(&state, &committed_config, outcome).await;
    }

    Ok(Json(api_types::ConfigSetupResponse { ok: true }))
}

async fn apply_config_outcome(
    state: &AppState,
    committed_config: &settings::Config,
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

fn parse_config_status_contents(
    contents: &str,
    config_path: &std::path::Path,
) -> Result<(), settings::ConfigError> {
    settings::parse_config(contents, config_path).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::parse_config_status_contents;

    #[test]
    fn config_status_accepts_config_from_before_default_task_agent() {
        let contents = r#"{
          "workspace": "~/.mando/workspace",
          "ui": { "openAtLogin": false },
          "features": {
            "scout": false,
            "setupDismissed": false,
            "claudeCodeVerified": true
          },
          "channels": { "telegram": { "enabled": false, "owner": "" } },
          "gateway": { "dashboard": { "host": "127.0.0.1", "port": 18791 } },
          "captain": {
            "autoSchedule": true,
            "autoMerge": false,
            "maxConcurrentWorkers": null,
            "tickIntervalS": 30,
            "tz": "UTC"
          },
          "scout": {
            "interests": { "high": [], "low": [] },
            "userContext": { "role": "", "knownDomains": [], "explainDomains": [] }
          },
          "env": {}
        }"#;

        parse_config_status_contents(contents, std::path::Path::new("config.json"))
            .expect("older config should inherit default task agent");
    }
}
