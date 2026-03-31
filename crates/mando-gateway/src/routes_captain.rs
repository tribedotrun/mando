//! /api/captain/* and /api/workers/* route handlers.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

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
pub(crate) async fn get_health_system(State(state): State<AppState>) -> Json<Value> {
    let config = state.config.load_full();
    let active_paths = state.runtime_paths.clone();
    let configured_paths = mando_config::resolve_captain_runtime_paths(&config);
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
    let data_dir = mando_config::data_dir();
    let linear_slug = state.linear_workspace_slug.read().await.clone();
    let uptime = state.start_time.elapsed().as_secs();
    Json(json!({
        "healthy": healthy,
        "version": env!("CARGO_PKG_VERSION"),
        "pid": std::process::id(),
        "uptime": uptime,
        "active_workers": active,
        "total_items": total,
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
        "linear_workspace_slug": linear_slug,
    }))
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

#[derive(Deserialize)]
pub(crate) struct MergeBody {
    pub pr_num: String,
    pub project: Option<String>,
}

/// POST /api/captain/merge
pub(crate) async fn post_captain_merge(
    State(state): State<AppState>,
    Json(body): Json<MergeBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let config = state.config.load_full();
    let github_repo = match &body.project {
        Some(r) => mando_config::resolve_project_config(Some(r), &config)
            .and_then(|(_, pc)| pc.github_repo.clone()),
        None => config
            .captain
            .projects
            .values()
            .next()
            .and_then(|pc| pc.github_repo.clone()),
    };
    let github_repo = match github_repo {
        Some(r) => r,
        None => {
            return Err(error_response(
                StatusCode::BAD_REQUEST,
                "no GitHub repo configured for this project — cannot merge",
            ))
        }
    };
    let store = state.task_store.read().await;
    match mando_captain::runtime::dashboard::merge_pr(&store, &body.pr_num, &github_repo).await {
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

#[derive(Deserialize)]
pub(crate) struct NudgeBody {
    pub item_id: String,
    pub message: String,
}

/// POST /api/captain/nudge — send a nudge message to a stuck worker.
pub(crate) async fn post_captain_nudge(
    State(state): State<AppState>,
    Json(body): Json<NudgeBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let id = body.item_id.parse::<i64>().map_err(|_| {
        error_response(
            StatusCode::BAD_REQUEST,
            &format!("invalid id: {}", body.item_id),
        )
    })?;
    let config = state.config.load_full();
    let workflow = state.captain_workflow.load_full();
    let notifier = crate::captain_notifier(&state, &config);
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

    mando_captain::runtime::action_contract::nudge_item(
        &mut item,
        Some(&body.message),
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
    state.bus.send(
        mando_types::BusEvent::Tasks,
        Some(json!({"action": "nudge", "id": id})),
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
    match mando_captain::io::process_manager::kill_worker_process(body.pid).await {
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
pub(crate) async fn get_workers(State(state): State<AppState>) -> Json<Value> {
    let config = state.config.load_full();
    let workflow = state.captain_workflow.load_full();
    let store = state.task_store.read().await;
    let all_items = store.load_all().await.unwrap_or_else(|e| {
        tracing::error!(error = %e, "failed to load tasks for workers endpoint");
        Vec::new()
    });
    drop(store);

    let health_path = mando_config::worker_health_path();
    let health = mando_captain::io::health_store::load_health_state(&health_path);
    let nudge_budget = workflow.agent.max_interventions;
    let stale_threshold_s = workflow.agent.stale_threshold_s;

    // Filter in-progress items with a worker — single load_all, no N+1 find_by_id.
    let workers: Vec<Value> = all_items
        .iter()
        .filter(|task| {
            (task.status == mando_types::task::ItemStatus::InProgress
                || task.status == mando_types::task::ItemStatus::CaptainReviewing)
                && task.worker.is_some()
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
            let github_repo = crate::resolve_github_repo(task.project.as_deref(), &config);
            let stream_stale_s: Option<f64> = task
                .session_ids
                .worker
                .as_deref()
                .map(mando_config::stream_path_for_session)
                .and_then(|p| mando_cc::stream_stale_seconds(&p));
            let process_alive = pid.is_some_and(mando_cc::is_process_alive);
            let is_stale = match (process_alive, stream_stale_s) {
                (true, Some(s)) => s >= stale_threshold_s,
                (true, None) => false, // just started, no stream yet
                (false, _) => true,    // dead process = stale
            };
            json!({
                "id": task.id,
                "title": task.title,
                "worker": task.worker,
                "project": task.project,
                "github_repo": github_repo,
                "worktree": task.worktree,
                "branch": task.branch,
                "pr": task.pr,
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

    Json(json!({ "workers": workers }))
}

/// GET /api/workers/{id}
pub(crate) async fn get_worker(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let config = state.config.load_full();
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
        store.find_by_id(idx.id).await.unwrap_or(None)
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
        Some(it) => {
            let github_repo = crate::resolve_github_repo(it.project.as_deref(), &config);
            Ok(Json(json!({
                "id": it.id,
                "title": it.title,
                "status": it.status,
                "worker": it.worker,
                "project": it.project,
                "github_repo": github_repo,
                "worktree": it.worktree,
                "branch": it.branch,
                "pr": it.pr,
                "started_at": it.worker_started_at,
                "last_activity_at": it.last_activity_at,
                "cc_session_id": it.session_ids.worker,
                "intervention_count": it.intervention_count,
            })))
        }
        None => Err(error_response(
            StatusCode::NOT_FOUND,
            &format!("worker {id} not found"),
        )),
    }
}
