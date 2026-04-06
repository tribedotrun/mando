//! Config management endpoints for the daemon.

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde_json::{json, Value};

use crate::AppState;

/// Load captain + scout workflows for a candidate config WITHOUT publishing
/// them to daemon state. Returns the loaded workflows on success so callers
/// can atomically commit both config and workflows together after validation.
///
/// This is the safe path: load everything first, fail the request if anything
/// is bad, then commit. Avoids the partial-apply window where config is live
/// but workflow reload has failed, leaving the daemon running mixed state.
fn load_workflows_for(
    state: &AppState,
    cfg: &mando_config::Config,
) -> anyhow::Result<(
    mando_config::workflow::CaptainWorkflow,
    mando_config::workflow::ScoutWorkflow,
)> {
    let mut new_cwf = mando_config::load_captain_workflow(
        &mando_config::captain_workflow_path(),
        cfg.captain.tick_interval_s,
    )?;
    let mut new_dwf = mando_config::load_scout_workflow(&mando_config::scout_workflow_path(), cfg)?;

    if state.dev_mode {
        crate::apply_dev_model_overrides(&mut new_cwf, &mut new_dwf);
    }

    Ok((new_cwf, new_dwf))
}

/// Publish a previously-loaded workflow pair to daemon state. Split from
/// `load_workflows_for` so callers can fail the request before any state
/// mutation if loading fails.
fn publish_workflows(
    state: &AppState,
    cwf: mando_config::workflow::CaptainWorkflow,
    dwf: mando_config::workflow::ScoutWorkflow,
) {
    state.captain_workflow.store(Arc::new(cwf));
    state.scout_workflow.store(Arc::new(dwf));
}

/// GET /api/config — read current config.
pub(crate) async fn get_config(State(state): State<AppState>) -> Result<Json<Value>, StatusCode> {
    let config = state.config.load_full();
    let val = serde_json::to_value(&*config).map_err(|e| {
        tracing::error!(error = %e, "failed to serialize config");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(val))
}

/// PUT /api/config — write config.json, hot-reload into daemon.
pub(crate) async fn put_config(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    // Validate by deserializing.
    let mut new_config: mando_config::Config = match serde_json::from_value(body) {
        Ok(c) => c,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": format!("invalid config: {e}")})),
            )
                .into_response();
        }
    };

    // Serialize config writes — prevents concurrent saves from clobbering each other.
    let _write_guard = state.config_write_mu.lock().await;

    // Populate runtime fields (e.g. Telegram tokens from env section).
    new_config.populate_runtime_fields();

    // Validate workflow config before persisting anything.
    {
        let tick_s = new_config.captain.tick_interval_s;
        if let Err(e) =
            mando_config::try_load_captain_workflow(&mando_config::captain_workflow_path(), tick_s)
        {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"error": e.to_string()})),
            )
                .into_response();
        }
    }

    // Load the workflow files against the candidate config BEFORE persisting
    // anything. If the workflow files are bad, we refuse the update with no
    // state mutation, rather than committing a config that leaves the daemon
    // running with mismatched workflows.
    let workflows = match load_workflows_for(&state, &new_config) {
        Ok(w) => w,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": format!("workflow reload failed: {e}")})),
            )
                .into_response();
        }
    };

    // Save to disk (validation passed). save_config uses blocking std::fs —
    // move it off the async runtime so we don't stall the executor while
    // holding config_write_mu.
    let save_config = new_config.clone();
    let save_result =
        tokio::task::spawn_blocking(move || mando_config::save_config(&save_config, None)).await;
    match save_result {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": format!("save failed: {e}")})),
            )
                .into_response();
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": format!("save task panicked: {e}")})),
            )
                .into_response();
        }
    }

    // Commit config and workflows together. Both are pre-validated, so
    // neither of these can fail.
    state.config.store(Arc::new(new_config));
    publish_workflows(&state, workflows.0, workflows.1);

    // Notify SSE clients.
    state.bus.send(mando_types::BusEvent::Status, None);

    let configured_paths = mando_config::resolve_captain_runtime_paths(&state.config.load_full());
    Json(json!({
        "ok": true,
        "restartRequired": state.runtime_paths != configured_paths,
        "taskDbPath": state.runtime_paths.task_db_path.to_string_lossy(),
        "workerHealthPath": state.runtime_paths.worker_health_path.to_string_lossy(),
        "lockfilePath": state.runtime_paths.lockfile_path.to_string_lossy(),
    }))
    .into_response()
}

/// GET /api/config/status — returns whether config exists and setup is complete.
pub(crate) async fn get_config_status(State(state): State<AppState>) -> Json<Value> {
    let config_path = mando_config::get_config_path();
    let exists = config_path.exists();
    let config = state.config.load_full();
    let active_paths = state.runtime_paths.clone();
    let configured_paths = mando_config::resolve_captain_runtime_paths(&config);
    let (setup_complete, error) = if exists {
        match tokio::fs::read_to_string(&config_path).await {
            Ok(contents) => match serde_json::from_str::<mando_config::Config>(&contents) {
                Ok(_) => (true, None),
                Err(e) => {
                    tracing::warn!(path = %config_path.display(), error = %e, "config.json exists but is corrupt");
                    (false, Some(format!("corrupt config: {e}")))
                }
            },
            Err(e) => {
                tracing::warn!(path = %config_path.display(), error = %e, "config.json exists but is unreadable");
                (false, Some(format!("unreadable: {e}")))
            }
        }
    } else {
        (false, None)
    };
    Json(json!({
        "exists": exists,
        "setupComplete": setup_complete,
        "error": error,
        "taskDbPath": active_paths.task_db_path.to_string_lossy(),
        "workerHealthPath": active_paths.worker_health_path.to_string_lossy(),
        "lockfilePath": active_paths.lockfile_path.to_string_lossy(),
        "configuredTaskDbPath": configured_paths.task_db_path.to_string_lossy(),
        "configuredWorkerHealthPath": configured_paths.worker_health_path.to_string_lossy(),
        "configuredLockfilePath": configured_paths.lockfile_path.to_string_lossy(),
        "restartRequired": active_paths != configured_paths,
    }))
}

/// POST /api/config/setup — mark first-launch setup complete.
pub(crate) async fn post_config_setup(
    State(state): State<AppState>,
    Json(body): Json<Value>,
) -> impl IntoResponse {
    if let Some(config_val) = body.get("config") {
        let mut new_config: mando_config::Config = match serde_json::from_value(config_val.clone())
        {
            Ok(c) => c,
            Err(e) => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error": format!("invalid config: {e}")})),
                )
                    .into_response();
            }
        };

        let _write_guard = state.config_write_mu.lock().await;
        new_config.populate_runtime_fields();

        // Validate before persisting. try_load_captain_workflow is the lighter
        // BAD_REQUEST check; load_workflows_for below is the full load that
        // also covers the scout workflow.
        {
            let tick_s = new_config.captain.tick_interval_s;
            if let Err(e) = mando_config::try_load_captain_workflow(
                &mando_config::captain_workflow_path(),
                tick_s,
            ) {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(json!({"error": e.to_string()})),
                )
                    .into_response();
            }
        }

        // Full load of both workflow files against the candidate config
        // before any state mutation. Refuses the setup if either fails.
        let workflows = match load_workflows_for(&state, &new_config) {
            Ok(w) => w,
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": format!("workflow reload failed: {e}")})),
                )
                    .into_response();
            }
        };

        let save_config = new_config.clone();
        let save_result =
            tokio::task::spawn_blocking(move || mando_config::save_config(&save_config, None))
                .await;
        match save_result {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": format!("save failed: {e}")})),
                )
                    .into_response();
            }
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(json!({"error": format!("save task panicked: {e}")})),
                )
                    .into_response();
            }
        }

        state.config.store(Arc::new(new_config));
        publish_workflows(&state, workflows.0, workflows.1);
    }

    Json(json!({"ok": true})).into_response()
}

/// GET /api/config/paths — returns config/runtime paths.
pub(crate) async fn get_config_paths(State(state): State<AppState>) -> Json<Value> {
    let config = state.config.load_full();
    let active_paths = state.runtime_paths.clone();
    let configured_paths = mando_config::resolve_captain_runtime_paths(&config);
    Json(json!({
        "dataDir": mando_config::data_dir().to_string_lossy(),
        "configPath": mando_config::get_config_path().to_string_lossy(),
        "taskDbPath": active_paths.task_db_path.to_string_lossy(),
        "workerHealthPath": active_paths.worker_health_path.to_string_lossy(),
        "lockfilePath": active_paths.lockfile_path.to_string_lossy(),
        "configuredTaskDbPath": configured_paths.task_db_path.to_string_lossy(),
        "configuredWorkerHealthPath": configured_paths.worker_health_path.to_string_lossy(),
        "configuredLockfilePath": configured_paths.lockfile_path.to_string_lossy(),
        "restartRequired": active_paths != configured_paths,
    }))
}
