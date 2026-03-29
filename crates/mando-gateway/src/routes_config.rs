//! Config management endpoints for the daemon.

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde_json::{json, Value};

use crate::AppState;

/// GET /api/config — read current config.
pub(crate) async fn get_config(State(state): State<AppState>) -> Json<Value> {
    let config = state.config.load_full();
    let val = serde_json::to_value(&*config).unwrap_or(json!({}));
    Json(val)
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

    // Save to disk.
    if let Err(e) = mando_config::save_config(&new_config, None) {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": format!("save failed: {e}")})),
        )
            .into_response();
    }

    // Hot-reload into daemon state.
    state.config.store(Arc::new(new_config));

    // Also reload workflows.
    {
        let new_cwf = mando_config::load_captain_workflow(&mando_config::captain_workflow_path());
        let clarifier_model = new_cwf.models.clarifier.clone();
        state.captain_workflow.store(Arc::new(new_cwf));

        let mut clarifier_mgr = state.clarifier_mgr.write().await;
        clarifier_mgr.set_default_model(&clarifier_model);
    }
    {
        let cfg = state.config.load_full();
        let new_dwf = mando_config::load_scout_workflow(&mando_config::scout_workflow_path(), &cfg);
        state.scout_workflow.store(Arc::new(new_dwf));
    }

    // Notify SSE clients.
    state.bus.send(mando_types::BusEvent::Status, None);

    Json(json!({"ok": true})).into_response()
}

/// GET /api/config/status — returns whether config exists and setup is complete.
pub(crate) async fn get_config_status() -> Json<Value> {
    let config_path = mando_config::get_config_path();
    let exists = config_path.exists();
    let (setup_complete, error) = if exists {
        match std::fs::read_to_string(&config_path) {
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
    Json(json!({ "exists": exists, "setupComplete": setup_complete, "error": error }))
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
        if let Err(e) = mando_config::save_config(&new_config, None) {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": format!("save failed: {e}")})),
            )
                .into_response();
        }
        state.config.store(Arc::new(new_config));

        // Reload workflows so scout picks up new config values.
        {
            let new_cwf =
                mando_config::load_captain_workflow(&mando_config::captain_workflow_path());
            let clarifier_model = new_cwf.models.clarifier.clone();
            state.captain_workflow.store(Arc::new(new_cwf));

            let mut clarifier_mgr = state.clarifier_mgr.write().await;
            clarifier_mgr.set_default_model(&clarifier_model);
        }
        {
            let cfg = state.config.load_full();
            let new_dwf =
                mando_config::load_scout_workflow(&mando_config::scout_workflow_path(), &cfg);
            state.scout_workflow.store(Arc::new(new_dwf));
        }
    }

    Json(json!({"ok": true})).into_response()
}

/// GET /api/config/paths — returns data dir and config path.
pub(crate) async fn get_config_paths() -> Json<Value> {
    Json(json!({
        "dataDir": mando_config::data_dir().to_string_lossy(),
        "configPath": mando_config::get_config_path().to_string_lossy(),
    }))
}
