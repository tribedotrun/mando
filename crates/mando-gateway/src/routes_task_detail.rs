//! GET /api/tasks/{id}/* detail route handlers.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

use crate::response::{error_response, internal_error};
use crate::AppState;

/// Resolve (repo, pr_number) from a PR ref + project name via config.
fn resolve_pr(
    pr: &str,
    project: Option<&str>,
    config: &mando_config::Config,
) -> Option<(String, u32)> {
    let num = mando_types::task::extract_pr_number(pr)?.parse().ok()?;
    Some((crate::resolve_github_repo(project, config)?, num))
}

/// Resolve a string ID to a numeric task ID.
fn resolve_task_id(id: &str) -> Result<i64, (StatusCode, Json<Value>)> {
    id.parse::<i64>()
        .map_err(|_| error_response(StatusCode::BAD_REQUEST, &format!("invalid task id: {id}")))
}

/// GET /api/tasks/{id}/history
pub(crate) async fn get_task_history(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let store = state.task_store.read().await;
    let task_id: i64 = resolve_task_id(&id)?;
    let pool = store.pool();

    let entries = mando_db::queries::ask_history::load(pool, task_id)
        .await
        .map_err(internal_error)?;

    Ok(Json(json!({ "history": entries })))
}

/// GET /api/tasks/{id}/timeline
pub(crate) async fn get_task_timeline(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let store = state.task_store.read().await;
    let id_num: i64 = resolve_task_id(&id)?;
    let full_item = store.find_by_id(id_num).await.map_err(internal_error)?;
    let pool = store.pool().clone();
    let item_ref = full_item.as_ref();

    match mando_captain::runtime::dashboard_timeline::get_item_timeline(&id, None, item_ref, &pool)
        .await
    {
        Ok(events) => {
            let count = events.as_array().map(|a| a.len()).unwrap_or(0);
            Ok(Json(json!({
                "id": id,
                "events": events,
                "count": count,
            })))
        }
        Err(e) => Err(error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &e.to_string(),
        )),
    }
}

/// GET /api/tasks/{id}/sessions
pub(crate) async fn get_task_sessions(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let _id_num: i64 = resolve_task_id(&id)?;
    let store = state.task_store.read().await;

    let sessions = store
        .list_sessions_for_task(&id)
        .await
        .map_err(internal_error)?;

    let matched: Vec<Value> = sessions
        .into_iter()
        .map(|e| {
            json!({
                "session_id": e.session_id,
                "status": e.status,
                "caller": e.caller,
                "started_at": e.created_at,
                "duration_ms": e.duration_ms,
                "cost_usd": e.cost_usd,
                "model": e.model,
                "resumed": e.resumed,
                "cwd": e.cwd,
                "worker_name": e.worker_name,
            })
        })
        .collect();

    let count = matched.len();
    Ok(Json(json!({ "sessions": matched, "count": count })))
}

/// GET /api/tasks/{id}/pr-summary
pub(crate) async fn get_task_pr_summary(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    // Read store, extract what we need, then drop the guard before network I/O.
    let (pr_ref, project) = {
        let store = state.task_store.read().await;
        let id_num: i64 = resolve_task_id(&id)?;
        let item = store
            .find_by_id(id_num)
            .await
            .map_err(internal_error)?
            .ok_or_else(|| {
                error_response(StatusCode::NOT_FOUND, &format!("item {id} not found"))
            })?;
        (item.pr.clone().unwrap_or_default(), item.project.clone())
    };

    // Fetch PR body outside the read lock.
    let config = state.config.load();
    let (summary, summary_error) =
        if let Some((repo, num)) = resolve_pr(&pr_ref, project.as_deref(), &config) {
            match mando_captain::io::github_pr::get_pr_body(&repo, num).await {
                Ok(body) if !body.is_empty() => (Some(body), None),
                Ok(_) => (None, None),
                Err(e) => {
                    tracing::warn!(
                        task_id = %id,
                        pr = %pr_ref,
                        error = %e,
                        "failed to fetch PR body from GitHub"
                    );
                    (None, Some(e.to_string()))
                }
            }
        } else {
            if !pr_ref.is_empty() {
                tracing::debug!(pr = %pr_ref, "cannot resolve PR repo, skipping body fetch");
            }
            (None, None)
        };

    Ok(Json(json!({
        "id": id,
        "pr": pr_ref,
        "summary": summary,
        "summary_error": summary_error,
    })))
}
