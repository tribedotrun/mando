//! /api/captain/* and /api/workers/* route handlers.

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;

use crate::response::{error_response, internal_error, ApiError};
use crate::AppState;

fn ui_desired_state_to_wire(state: transport_ui::UiDesiredState) -> api_types::UiDesiredState {
    match state {
        transport_ui::UiDesiredState::Running => api_types::UiDesiredState::Running,
        transport_ui::UiDesiredState::Suppressed => api_types::UiDesiredState::Suppressed,
        transport_ui::UiDesiredState::Updating => api_types::UiDesiredState::Updating,
    }
}

/// GET /api/health — lightweight liveness probe (public, no auth).
pub(crate) async fn get_health(State(state): State<AppState>) -> Json<api_types::HealthResponse> {
    let uptime = state.start_time.elapsed().as_secs();
    Json(api_types::HealthResponse {
        healthy: true,
        version: env!("CARGO_PKG_VERSION").to_string(),
        pid: std::process::id(),
        uptime,
    })
}

/// GET /api/health/system — full system info (protected, auth required).
///
/// Returns HTTP 503 if the underlying database is unreachable or the
/// captain auto-tick loop has flagged itself as degraded.
pub(crate) async fn get_health_system(
    State(state): State<AppState>,
) -> (StatusCode, Json<api_types::SystemHealthResponse>) {
    let config = state.settings.load_config();
    let active_paths = state.runtime_paths.clone();
    let configured_paths = captain::resolve_captain_runtime_paths(&config);
    let ui_status = state.ui_runtime.status().await;
    let telegram_status = state.telegram_runtime.status().await;
    let mut healthy = true;
    let (active, total) = state
        .captain
        .health_summary_counts()
        .await
        .unwrap_or_else(|e| {
            tracing::error!(module = "transport-http-transport-routes_captain", error = %e, "failed to load captain health summary");
            healthy = false;
            (0, 0)
        });
    let captain_degraded = state.captain.health_degraded();
    if captain_degraded {
        healthy = false;
    }
    if telegram_status.enabled && !telegram_status.running {
        healthy = false;
    }
    let data_dir = global_infra::paths::data_dir();
    let uptime = state.start_time.elapsed().as_secs();
    let status = if healthy {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (
        status,
        Json(api_types::SystemHealthResponse {
            healthy,
            version: env!("CARGO_PKG_VERSION").to_string(),
            pid: std::process::id(),
            uptime,
            active_workers: active,
            total_items: total,
            captain_degraded,
            projects: config
                .captain
                .projects
                .values()
                .map(|pc| pc.name.clone())
                .collect(),
            data_dir: data_dir.to_string_lossy().to_string(),
            config_path: settings::config::get_config_path()
                .to_string_lossy()
                .to_string(),
            task_db_path: active_paths.task_db_path.to_string_lossy().to_string(),
            worker_health_path: active_paths
                .worker_health_path
                .to_string_lossy()
                .to_string(),
            lockfile_path: active_paths.lockfile_path.to_string_lossy().to_string(),
            configured_task_db_path: configured_paths.task_db_path.to_string_lossy().to_string(),
            configured_worker_health_path: configured_paths
                .worker_health_path
                .to_string_lossy()
                .to_string(),
            configured_lockfile_path: configured_paths.lockfile_path.to_string_lossy().to_string(),
            restart_required: active_paths != configured_paths,
            telegram: api_types::TelegramHealth {
                enabled: telegram_status.enabled,
                running: telegram_status.running,
                owner: telegram_status.owner,
                last_error: telegram_status.last_error,
                degraded: telegram_status.degraded,
                restart_count: u64::from(telegram_status.restart_count),
                mode: telegram_status.mode.to_string(),
            },
            ui: api_types::UiHealthResponse {
                desired_state: ui_desired_state_to_wire(ui_status.desired_state),
                current_pid: ui_status.current_pid,
                launch_available: ui_status.launch_available,
                running: ui_status.running,
                last_error: ui_status.last_error,
                degraded: ui_status.degraded,
                restart_count: ui_status.restart_count,
            },
        }),
    )
}

/// POST /api/captain/tick
#[crate::instrument_api(method = "POST", path = "/api/captain/tick")]
pub(crate) async fn post_captain_tick(
    State(state): State<AppState>,
    Json(body): Json<api_types::TickRequest>,
) -> Result<Json<api_types::TickResult>, ApiError> {
    let workflow = state.settings.load_captain_workflow();
    let dry_run = body.dry_run.unwrap_or(false);
    let emit_notifications = body.emit_notifications.unwrap_or(true);
    match state
        .captain
        .trigger_captain_tick(&workflow, dry_run, emit_notifications)
        .await
    {
        Ok(result) => {
            let val = serde_json::to_value(&result)
                .map_err(|e| internal_error(e, "failed to serialize tick result"))?;
            Ok(Json(serde_json::from_value(val).map_err(|e| {
                internal_error(e, "failed to decode tick result")
            })?))
        }
        Err(e) => Err(internal_error(e, "captain tick failed")),
    }
}

/// POST /api/captain/triage
#[crate::instrument_api(method = "POST", path = "/api/captain/triage")]
pub(crate) async fn post_captain_triage(
    State(state): State<AppState>,
    Json(body): Json<api_types::TriageRequest>,
) -> Result<Json<api_types::TriageResponse>, ApiError> {
    match state
        .captain
        .triage_pending_review(body.item_id.as_deref())
        .await
    {
        Ok(val) => Ok(Json(val)),
        Err(e) => Err(internal_error(e, "triage failed")),
    }
}

/// POST /api/captain/stop
#[crate::instrument_api(method = "POST", path = "/api/captain/stop")]
pub(crate) async fn post_captain_stop(
    State(state): State<AppState>,
    Json(_body): Json<api_types::EmptyRequest>,
) -> Result<Json<api_types::StopWorkersResponse>, ApiError> {
    match state.captain.stop_all_workers().await {
        Ok(killed) => Ok(Json(api_types::StopWorkersResponse {
            killed: killed as usize,
        })),
        Err(e) => Err(internal_error(e, "failed to stop workers")),
    }
}

/// POST /api/captain/nudge (JSON or multipart with optional images)
#[crate::instrument_api(method = "POST", path = "/api/captain/nudge")]
pub(crate) async fn post_captain_nudge(
    State(state): State<AppState>,
    request: axum::extract::Request,
) -> Result<Json<api_types::NudgeResponse>, ApiError> {
    let body = crate::image_upload_ext::extract_nudge(request).await?;
    let result = post_captain_nudge_inner(&state, &body).await;
    if result.is_err() {
        crate::image_upload::cleanup_saved_images(&body.saved_images).await;
    }
    result
}

async fn post_captain_nudge_inner(
    state: &AppState,
    body: &crate::image_upload::NudgeWithImages,
) -> Result<Json<api_types::NudgeResponse>, ApiError> {
    let id = body.item_id.parse::<i64>().map_err(|_| {
        error_response(
            StatusCode::BAD_REQUEST,
            &format!("invalid id: {}", body.item_id),
        )
    })?;
    let workflow = state.settings.load_captain_workflow();
    let config = state.settings.load_config();
    let notifier = crate::captain_notifier(state, &config);
    let mut item = state
        .captain
        .load_task(id)
        .await
        .map_err(|e| internal_error(e, "failed to load task"))?
        .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "item not found"))?;
    let worker_name = item
        .worker
        .clone()
        .ok_or_else(|| error_response(StatusCode::BAD_REQUEST, "item has no worker"))?;
    let mut alerts = Vec::new();

    let message = if body.saved_images.is_empty() {
        body.message.clone()
    } else {
        format!(
            "{}{}",
            body.message,
            crate::image_upload::format_image_paths(&body.saved_images)
        )
    };

    state
        .captain
        .nudge_item(&mut item, Some(&message), &workflow, &notifier, &mut alerts)
        .await
        .map_err(|e| internal_error(e, "nudge failed"))?;

    state
        .captain
        .write_task(&item)
        .await
        .map_err(|e| internal_error(e, "failed to save task"))?;

    if !body.saved_images.is_empty() {
        if let Err(e) = state
            .captain
            .append_task_images(id, &body.saved_images)
            .await
        {
            tracing::warn!(module = "transport-http-transport-routes_captain", task_id = id, error = ?e, "failed to persist nudge images");
        }
    }

    let updated_val = state.captain.task_json(id).await.ok().flatten();
    let task_item: Option<api_types::TaskItem> = updated_val
        .as_ref()
        .and_then(|v| serde_json::from_value(v.clone()).ok());
    state.bus.send(global_bus::BusPayload::Tasks(Some(
        api_types::TaskEventData {
            action: Some("updated".into()),
            item: task_item.clone(),
            id: Some(id),
            cleared_by: None,
        },
    )));
    let cc_sid = task_item
        .as_ref()
        .and_then(|t| t.session_ids.as_ref())
        .and_then(|s| s.worker.as_deref())
        .unwrap_or("");
    let pid = state.captain.resolve_worker_pid(cc_sid, &worker_name);

    Ok(Json(api_types::NudgeResponse {
        ok: true,
        worker: Some(worker_name),
        pid,
        status: task_item.as_ref().and_then(|t| {
            serde_json::to_value(t.status)
                .ok()
                .and_then(|v| v.as_str().map(|s| s.to_string()))
        }),
        alerts: Some(alerts),
    }))
}

/// GET /api/workers
#[crate::instrument_api(method = "GET", path = "/api/workers")]
pub(crate) async fn get_workers(
    State(state): State<AppState>,
) -> Result<Json<api_types::WorkersResponse>, ApiError> {
    let workflow = state.settings.load_captain_workflow();
    let workers = state
        .captain
        .workers_dashboard(&workflow)
        .await
        .map_err(|e| internal_error(e, "failed to load worker dashboard"))?;
    let workers = serde_json::from_value::<Vec<api_types::WorkerDetail>>(
        serde_json::to_value(workers)
            .map_err(|e| internal_error(e, "failed to serialize workers"))?,
    )
    .map_err(|e| internal_error(e, "failed to convert workers to api type"))?;
    let rl_remaining = effective_rate_limit_remaining_secs(&state).await;
    Ok(Json(api_types::WorkersResponse {
        workers,
        rate_limit_remaining_secs: Some(rl_remaining),
    }))
}

/// Effective remaining cooldown seconds for the UI. Resolves to:
/// - 0 when at least one credential can spawn (or no cooldown is active),
/// - the earliest credential cooldown when credentials exist but all are cooling down,
/// - the ambient cooldown when no credentials are configured.
async fn effective_rate_limit_remaining_secs(state: &AppState) -> u64 {
    let has_credentials = state.settings.has_any_credentials().await.unwrap_or(false);
    if !has_credentials {
        return state.captain.ambient_rate_limit_remaining_secs();
    }
    let available = state
        .settings
        .pick_worker_credential(None)
        .await
        .unwrap_or(None)
        .is_some();
    if available {
        return 0;
    }
    state
        .settings
        .earliest_credential_cooldown_remaining_secs()
        .await
        .max(0) as u64
}
