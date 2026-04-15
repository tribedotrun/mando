//! GET /api/tasks/{id}/* detail route handlers.

use std::collections::HashMap;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

use mando_db::caller::SessionCaller;

use crate::response::{error_response, internal_error};
use crate::AppState;

/// Resolve (repo, pr_number) from the task's integer PR number + github_repo.
fn resolve_pr(pr_number: i64, github_repo: Option<&str>) -> Option<(String, u32)> {
    let num: u32 = pr_number.try_into().ok()?;
    Some((github_repo?.to_string(), num))
}

/// Resolve a string ID to a numeric task ID.
fn resolve_task_id(id: &str) -> Result<i64, (StatusCode, Json<Value>)> {
    id.parse::<i64>()
        .map_err(|_| error_response(StatusCode::BAD_REQUEST, &format!("invalid task id: {id}")))
}

/// GET /api/tasks/{id}/artifacts
pub(crate) async fn get_task_artifacts(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let task_id = resolve_task_id(&id)?;
    let pool = state.db.pool();

    let artifacts = mando_db::queries::artifacts::list_for_task(pool, task_id)
        .await
        .map_err(|e| internal_error(e, "failed to load task artifacts"))?;

    Ok(Json(json!({ "artifacts": artifacts })))
}

/// GET /api/tasks/{id}/feed
///
/// Unified feed: merges timeline events, artifacts, and ask history into
/// a single chronologically-ordered stream.
pub(crate) async fn get_task_feed(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let task_id = resolve_task_id(&id)?;
    let store = state.task_store.read().await;
    let item = store
        .find_by_id(task_id)
        .await
        .map_err(|e| internal_error(e, "failed to load task"))?;
    if item.is_none() {
        return Err(error_response(
            StatusCode::NOT_FOUND,
            &format!("task {task_id} not found"),
        ));
    }
    let pool = store.pool().clone();
    drop(store);

    // Load all three data sources in parallel.
    let (timeline_result, artifacts_result, history_result) = tokio::join!(
        mando_captain::runtime::dashboard_timeline::get_item_timeline(
            &id,
            None,
            item.as_ref(),
            &pool,
        ),
        mando_db::queries::artifacts::list_for_task(&pool, task_id),
        mando_db::queries::ask_history::load(&pool, task_id),
    );

    let timeline_events =
        timeline_result.map_err(|e| internal_error(e, "failed to load task timeline"))?;
    let artifacts =
        artifacts_result.map_err(|e| internal_error(e, "failed to load task artifacts"))?;
    let history = history_result.map_err(|e| internal_error(e, "failed to load ask history"))?;

    // Build unified feed items with a type discriminator.
    let mut feed: Vec<Value> = Vec::new();

    // Build lookups for labeling human messages as reopen/rework:
    //   intent_by_ask      -- exact join via ask_id (post-fix events)
    //   intent_by_content  -- fallback join via message text. Populated from
    //                         HumanAsk intent metadata when present, plus
    //                         HumanReopen / ReworkRequested event content so
    //                         legacy rows (no ask_id, no intent on HumanAsk)
    //                         still match.
    let mut intent_by_ask: HashMap<String, String> = HashMap::new();
    let mut intent_by_content: HashMap<String, String> = HashMap::new();
    if let Some(events) = timeline_events.as_array() {
        for event in events {
            let event_type = event
                .get("event_type")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let data = event.get("data");
            match event_type {
                "human_ask" => {
                    let intent = data
                        .and_then(|d| d.get("intent"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    if intent.is_empty() || intent == "ask" {
                        continue;
                    }
                    if let Some(ask_id) =
                        data.and_then(|d| d.get("ask_id")).and_then(|v| v.as_str())
                    {
                        intent_by_ask.insert(ask_id.to_string(), intent.to_string());
                    }
                    if let Some(q) = data
                        .and_then(|d| d.get("question"))
                        .and_then(|v| v.as_str())
                    {
                        intent_by_content
                            .entry(q.to_string())
                            .or_insert_with(|| intent.to_string());
                    }
                }
                "human_reopen" | "rework_requested" => {
                    let inferred = if event_type == "rework_requested" {
                        "rework"
                    } else {
                        "reopen"
                    };
                    if let Some(c) = data.and_then(|d| d.get("content")).and_then(|v| v.as_str()) {
                        intent_by_content
                            .entry(c.to_string())
                            .or_insert_with(|| inferred.to_string());
                    }
                }
                _ => {}
            }
        }
    }

    // Timeline events.
    if let Some(events) = timeline_events.as_array() {
        for event in events {
            let ts = event
                .get("timestamp")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            feed.push(json!({
                "type": "timeline",
                "timestamp": ts,
                "data": event,
            }));
        }
    }

    // Artifacts.
    for artifact in &artifacts {
        feed.push(json!({
            "type": "artifact",
            "timestamp": artifact.created_at,
            "data": artifact,
        }));
    }

    // Ask history / advisor messages. Inject intent on human entries whose
    // ask_id (or, for legacy rows, question content) matches a reopen/rework
    // HumanAsk timeline event.
    for entry in &history {
        let mut data = serde_json::to_value(entry).unwrap_or_else(|_| json!({}));
        if entry.role == "human" {
            let intent = intent_by_ask
                .get(&entry.ask_id)
                .or_else(|| intent_by_content.get(&entry.content));
            if let Some(intent) = intent {
                if let Some(obj) = data.as_object_mut() {
                    obj.insert("intent".into(), Value::String(intent.clone()));
                }
            }
        }
        feed.push(json!({
            "type": "message",
            "timestamp": entry.timestamp,
            "data": data,
        }));
    }

    // Sort by timestamp.
    feed.sort_by(|a, b| {
        let ts_a = a.get("timestamp").and_then(|v| v.as_str()).unwrap_or("");
        let ts_b = b.get("timestamp").and_then(|v| v.as_str()).unwrap_or("");
        ts_a.cmp(ts_b)
    });

    Ok(Json(json!({
        "id": id,
        "feed": feed,
        "count": feed.len(),
    })))
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
        .map_err(|e| internal_error(e, "failed to load ask history"))?;

    Ok(Json(json!({ "history": entries })))
}

/// GET /api/tasks/{id}/timeline
pub(crate) async fn get_task_timeline(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let store = state.task_store.read().await;
    let id_num: i64 = resolve_task_id(&id)?;
    let full_item = store
        .find_by_id(id_num)
        .await
        .map_err(|e| internal_error(e, "failed to load task"))?;
    let pool = store.pool().clone();
    let item_ref = full_item.as_ref();

    let events =
        mando_captain::runtime::dashboard_timeline::get_item_timeline(&id, None, item_ref, &pool)
            .await
            .map_err(|e| internal_error(e, "failed to load task timeline"))?;

    let count = events.as_array().map(|a| a.len()).unwrap_or(0);
    Ok(Json(json!({
        "id": id,
        "events": events,
        "count": count,
    })))
}

/// GET /api/tasks/{id}/sessions?caller=workers
pub(crate) async fn get_task_sessions(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(params): Query<crate::routes_sessions::SessionsQuery>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let id_num: i64 = resolve_task_id(&id)?;
    let store = state.task_store.read().await;

    let sessions = store
        .list_sessions_for_task(id_num)
        .await
        .map_err(|e| internal_error(e, "failed to load task sessions"))?;

    let caller_filter = params.caller.as_deref().or(params.category.as_deref());

    let matched: Vec<Value> = sessions
        .into_iter()
        .filter(|e| match caller_filter {
            Some(filter) => {
                SessionCaller::parse(&e.caller).is_some_and(|c| c.group().as_str() == filter)
            }
            None => true,
        })
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
    let (pr_number, github_repo) = {
        let store = state.task_store.read().await;
        let id_num: i64 = resolve_task_id(&id)?;
        let item = store
            .find_by_id(id_num)
            .await
            .map_err(|e| internal_error(e, "failed to load task"))?
            .ok_or_else(|| {
                error_response(StatusCode::NOT_FOUND, &format!("item {id} not found"))
            })?;
        (item.pr_number, item.github_repo.clone())
    };

    // Fetch PR body outside the read lock.
    let (summary, summary_error) = if let Some(pr_num) = pr_number {
        if let Some((repo, num)) = resolve_pr(pr_num, github_repo.as_deref()) {
            match mando_captain::io::github_pr::get_pr_body(&repo, num).await {
                Ok(body) if !body.is_empty() => (Some(body), None),
                Ok(_) => (None, None),
                Err(e) => {
                    tracing::warn!(
                        task_id = %id,
                        pr_number = pr_num,
                        error = %e,
                        "failed to fetch PR body from GitHub"
                    );
                    (None, Some(e.to_string()))
                }
            }
        } else {
            tracing::debug!(
                pr_number = pr_num,
                "cannot resolve PR repo, skipping body fetch"
            );
            (None, None)
        }
    } else {
        (None, None)
    };

    // Work summary artifacts are now created by the CLI (mando todo summary).
    // This endpoint only fetches the PR body for display.

    Ok(Json(json!({
        "id": id,
        "pr_number": pr_number,
        "summary": summary,
        "summary_error": summary_error,
    })))
}
