//! /api/captain/* and /api/workers/* route handlers.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::background_tasks::captain_health_degraded;
use crate::response::{error_response, internal_error};
use crate::AppState;

/// GET /api/health — lightweight liveness probe (public, no auth).
pub(crate) async fn get_health(State(state): State<AppState>) -> Json<Value> {
    let uptime = state.start_time.elapsed().as_secs();
    Json(json!({
        "healthy": true,
        "version": env!("CARGO_PKG_VERSION"),
        "pid": std::process::id(),
        "uptime": uptime,
    }))
}

/// GET /api/health/system — full system info (protected, auth required).
///
/// Returns HTTP 503 if the underlying database is unreachable or the
/// captain auto-tick loop has flagged itself as degraded.
pub(crate) async fn get_health_system(State(state): State<AppState>) -> impl IntoResponse {
    let config = state.config.load_full();
    let active_paths = state.runtime_paths.clone();
    let configured_paths = mando_config::resolve_captain_runtime_paths(&config);
    let ui_status = state.ui_runtime.status().await;
    let telegram_status = state.telegram_runtime.status().await;
    let store = state.task_store.read().await;
    let mut healthy = true;
    let active = store.active_worker_count().await.unwrap_or_else(|e| {
        tracing::error!(error = %e, "failed to count active workers");
        healthy = false;
        0
    });
    let total = store
        .routing()
        .await
        .unwrap_or_else(|e| {
            tracing::error!(error = %e, "failed to load routing table");
            healthy = false;
            Vec::new()
        })
        .len();
    let captain_degraded = captain_health_degraded();
    if captain_degraded {
        healthy = false;
    }
    if telegram_status.enabled && !telegram_status.running {
        healthy = false;
    }
    let data_dir = mando_config::data_dir();
    let uptime = state.start_time.elapsed().as_secs();
    let body = json!({
        "healthy": healthy,
        "version": env!("CARGO_PKG_VERSION"),
        "pid": std::process::id(),
        "uptime": uptime,
        "active_workers": active,
        "total_items": total,
        "captain_degraded": captain_degraded,
        "projects": config.captain.projects.values().map(|pc| &pc.name).collect::<Vec<_>>(),
        "dataDir": data_dir.to_string_lossy(),
        "configPath": mando_config::get_config_path().to_string_lossy(),
        "taskDbPath": active_paths.task_db_path.to_string_lossy(),
        "workerHealthPath": active_paths.worker_health_path.to_string_lossy(),
        "lockfilePath": active_paths.lockfile_path.to_string_lossy(),
        "configuredTaskDbPath": configured_paths.task_db_path.to_string_lossy(),
        "configuredWorkerHealthPath": configured_paths.worker_health_path.to_string_lossy(),
        "configuredLockfilePath": configured_paths.lockfile_path.to_string_lossy(),
        "restartRequired": active_paths != configured_paths,
        "telegram": telegram_status,
        "ui": ui_status,
    });
    let status = if healthy {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };
    (status, Json(body))
}

#[derive(Deserialize)]
pub(crate) struct TickBody {
    #[serde(default)]
    pub dry_run: bool,
    #[serde(default = "default_emit_notifications")]
    pub emit_notifications: bool,
}

fn default_emit_notifications() -> bool {
    true
}

/// POST /api/captain/tick
pub(crate) async fn post_captain_tick(
    State(state): State<AppState>,
    Json(body): Json<TickBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let config = state.config.load_full();
    let workflow = state.captain_workflow.load_full();
    match mando_captain::runtime::dashboard::trigger_captain_tick(
        &config,
        &workflow,
        body.dry_run,
        Some(&state.bus),
        body.emit_notifications,
        &state.task_store,
        &state.cancellation_token,
    )
    .await
    {
        Ok(val) => Ok(Json(val)),
        Err(e) => Err(error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &e.to_string(),
        )),
    }
}

#[derive(Deserialize)]
pub(crate) struct TriageBody {
    pub item_id: Option<String>,
}

/// POST /api/captain/triage
pub(crate) async fn post_captain_triage(
    State(state): State<AppState>,
    Json(body): Json<TriageBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let config = state.config.load_full();
    let store = state.task_store.read().await;
    match mando_captain::runtime::dashboard_triage::triage_pending_review(
        &config,
        &store,
        body.item_id.as_deref(),
    )
    .await
    {
        Ok(val) => Ok(Json(val)),
        Err(e) => Err(error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &e.to_string(),
        )),
    }
}

/// POST /api/captain/stop
pub(crate) async fn post_captain_stop(
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let store = state.task_store.read().await;
    let pool = state.db.pool();
    match mando_captain::runtime::dashboard::stop_all_workers(&store, pool).await {
        Ok(killed) => Ok(Json(json!({"killed": killed}))),
        Err(e) => Err(error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &e.to_string(),
        )),
    }
}

/// POST /api/captain/nudge (JSON or multipart with optional images)
pub(crate) async fn post_captain_nudge(
    State(state): State<AppState>,
    request: axum::extract::Request,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
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
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let id = body.item_id.parse::<i64>().map_err(|_| {
        error_response(
            StatusCode::BAD_REQUEST,
            &format!("invalid id: {}", body.item_id),
        )
    })?;
    let config = state.config.load_full();
    let workflow = state.captain_workflow.load_full();
    let notifier = crate::captain_notifier(state, &config);
    let store = state.task_store.read().await;
    let mut item = store
        .find_by_id(id)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "item not found"))?;
    let worker_name = item
        .worker
        .clone()
        .ok_or_else(|| error_response(StatusCode::BAD_REQUEST, "item has no worker"))?;
    let mut alerts = Vec::new();

    // Embed image paths in the nudge message so the CC session can read them.
    let message = if body.saved_images.is_empty() {
        body.message.clone()
    } else {
        format!(
            "{}{}",
            body.message,
            crate::image_upload::format_image_paths(&body.saved_images)
        )
    };

    mando_captain::runtime::action_contract::nudge_item(
        &mut item,
        Some(&message),
        None, // manual nudge -- no circuit breaker reason
        &config,
        &workflow,
        &notifier,
        &mut alerts,
        store.pool(),
    )
    .await
    .map_err(|e| {
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("nudge failed: {e}"),
        )
    })?;

    store.write_task(&item).await.map_err(internal_error)?;

    // Persist images to the task after nudge succeeds.
    if !body.saved_images.is_empty() {
        if let Err(e) =
            crate::image_upload::append_task_images(&store, id, &body.saved_images).await
        {
            tracing::warn!(task_id = id, error = ?e, "failed to persist nudge images");
        }
    }

    // Re-read the task so the SSE event includes the updated images field.
    let updated = store
        .find_by_id(id)
        .await
        .ok()
        .flatten()
        .map(|t| serde_json::to_value(&t).unwrap());
    state.bus.send(
        mando_types::BusEvent::Tasks,
        Some(json!({"action": "updated", "item": updated, "id": id})),
    );
    let cc_sid = item.session_ids.worker.as_deref().unwrap_or("");
    let pid = mando_captain::io::pid_lookup::resolve_pid(cc_sid, &worker_name);

    Ok(Json(json!({
        "ok": true,
        "worker": worker_name,
        "pid": pid,
        "status": item.status.as_str(),
        "alerts": alerts,
    })))
}

#[derive(Deserialize)]
pub(crate) struct KillWorkerBody {
    pub pid: u32,
}

/// POST /api/workers/{id}/kill
pub(crate) async fn post_worker_kill(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<KillWorkerBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    match mando_captain::io::process_manager::kill_worker_process(mando_types::Pid::new(body.pid))
        .await
    {
        Ok(()) => {
            state.bus.send(mando_types::BusEvent::Tasks, None);
            Ok(Json(json!({"ok": true, "killed": id})))
        }
        Err(e) => Err(error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &e.to_string(),
        )),
    }
}

/// GET /api/workers
pub(crate) async fn get_workers(
    State(state): State<AppState>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let workflow = state.captain_workflow.load_full();
    let store = state.task_store.read().await;
    let all_items = store.load_all().await.map_err(|e| {
        tracing::error!(error = %e, "failed to load tasks for workers endpoint");
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("database error: {e}"),
        )
    })?;
    drop(store);

    let health_path = mando_config::worker_health_path();
    let health = mando_captain::io::health_store::load_health_state(&health_path).map_err(|e| {
        tracing::error!(error = %e, "failed to load health state for workers endpoint");
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("health state error: {e}"),
        )
    })?;
    let nudge_budget = workflow.agent.max_interventions;
    let stale_threshold_s = workflow.agent.stale_threshold_s.as_secs_f64();

    // Filter items with an active worker — single load_all, no N+1 find_by_id.
    let workers: Vec<Value> = all_items
        .iter()
        .filter(|task| {
            matches!(
                task.status,
                mando_types::task::ItemStatus::InProgress
                    | mando_types::task::ItemStatus::CaptainReviewing
                    | mando_types::task::ItemStatus::CaptainMerging
            ) && task.worker.is_some()
        })
        .map(|task| {
            let worker_name = task.worker.as_deref().unwrap_or("");
            let nudge_count = mando_captain::io::health_store::get_health_u32(
                &health,
                worker_name,
                "nudge_count",
            );
            let cc_sid = task.session_ids.worker.as_deref().unwrap_or("");
            let pid = mando_captain::io::pid_lookup::resolve_pid(cc_sid, worker_name);
            let last_action = health
                .get(worker_name)
                .and_then(|v| v.get("last_action"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let github_repo = task.github_repo.as_deref();
            let stream_stale_s: Option<f64> = task
                .session_ids
                .worker
                .as_deref()
                .map(mando_config::stream_path_for_session)
                .and_then(|p| mando_cc::stream_stale_seconds(&p));
            let process_alive = pid.is_some_and(mando_cc::is_process_alive);
            // During CaptainReviewing/CaptainMerging the worker process has
            // exited naturally — captain is handling the task via a separate
            // session. A dead worker in these states is expected, not stale.
            let in_captain_phase = matches!(
                task.status,
                mando_types::task::ItemStatus::CaptainReviewing
                    | mando_types::task::ItemStatus::CaptainMerging
            );
            let is_stale = if in_captain_phase {
                false
            } else {
                match (process_alive, stream_stale_s) {
                    (true, Some(s)) => s >= stale_threshold_s,
                    (true, None) => false, // just started, no stream yet
                    (false, _) => true,    // dead process = stale
                }
            };
            json!({
                "id": task.id,
                "title": task.title,
                "status": task.status.as_str(),
                "worker": task.worker,
                "project": task.project,
                "github_repo": github_repo,
                "worktree": task.worktree,
                "branch": task.branch,
                "pr_number": task.pr_number,
                "started_at": task.worker_started_at,
                "last_activity_at": task.last_activity_at,
                "cc_session_id": task.session_ids.worker,
                "intervention_count": task.intervention_count,
                "nudge_count": nudge_count,
                "nudge_budget": nudge_budget,
                "last_action": last_action,
                "pid": pid,
                "is_stale": is_stale,
            })
        })
        .collect();

    let rl_remaining = effective_rate_limit_remaining_secs(&state).await;
    Ok(Json(
        json!({ "workers": workers, "rate_limit_remaining_secs": rl_remaining }),
    ))
}

/// Effective remaining cooldown seconds for the UI. Resolves to:
/// - 0 when at least one credential can spawn (or no cooldown is active),
/// - the earliest credential cooldown when credentials exist but all are cooling down,
/// - the ambient cooldown when no credentials are configured.
async fn effective_rate_limit_remaining_secs(state: &AppState) -> u64 {
    let pool = state.db.pool();
    let has_credentials = mando_db::queries::credentials::has_any(pool)
        .await
        .unwrap_or(false);
    if !has_credentials {
        return mando_captain::runtime::ambient_rate_limit::remaining_secs();
    }
    let available = mando_db::queries::credentials::pick_for_worker(pool, None)
        .await
        .unwrap_or(None)
        .is_some();
    if available {
        return 0;
    }
    mando_db::queries::credentials::earliest_cooldown_remaining_secs(pool)
        .await
        .max(0) as u64
}

/// GET /api/workers/{id}
pub(crate) async fn get_worker(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let store = state.task_store.read().await;

    // Search by worker name, cc_session_id, or item id across indices + details.
    let routing = store.routing().await.map_err(|e| {
        tracing::error!(error = %e, "failed to load routing table");
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("database error: {e}"),
        )
    })?;
    let found = routing
        .iter()
        .find(|idx| idx.worker.as_deref() == Some(id.as_str()) || idx.id.to_string() == id);

    let full_item = if let Some(idx) = found {
        store.find_by_id(idx.id).await.map_err(|e| {
            tracing::error!(error = %e, "failed to load worker task detail");
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("database error: {e}"),
            )
        })?
    } else {
        store
            .load_all()
            .await
            .map_err(|e| {
                tracing::error!(error = %e, "failed to load tasks for worker lookup");
                error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    &format!("database error: {e}"),
                )
            })?
            .into_iter()
            .find(|t| t.session_ids.worker.as_deref() == Some(id.as_str()))
    };

    match full_item {
        Some(it) => Ok(Json(json!({
            "id": it.id,
            "title": it.title,
            "status": it.status,
            "worker": it.worker,
            "project": it.project,
            "github_repo": it.github_repo,
            "worktree": it.worktree,
            "branch": it.branch,
            "pr_number": it.pr_number,
            "started_at": it.worker_started_at,
            "last_activity_at": it.last_activity_at,
            "cc_session_id": it.session_ids.worker,
            "intervention_count": it.intervention_count,
        }))),
        None => Err(error_response(
            StatusCode::NOT_FOUND,
            &format!("worker {id} not found"),
        )),
    }
}
