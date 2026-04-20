//! GET /api/tasks/{id}/* detail route handlers.

use std::collections::HashMap;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::de::DeserializeOwned;

use sessions::SessionCaller;

use crate::response::{error_response, internal_error, ApiError};
use crate::AppState;

/// Resolve (repo, pr_number) from the task's integer PR number + github_repo.
fn feed_timestamp(item: &api_types::FeedItem) -> &str {
    match item {
        api_types::FeedItem::Timeline { timestamp, .. } => timestamp,
        api_types::FeedItem::Artifact { timestamp, .. } => timestamp,
        api_types::FeedItem::Message { timestamp, .. } => timestamp,
    }
}

fn resolve_pr(pr_number: i64, github_repo: Option<&str>) -> Option<(String, u32)> {
    let num: u32 = pr_number.try_into().ok()?;
    Some((github_repo?.to_string(), num))
}

fn wire<T: DeserializeOwned>(
    value: impl serde::Serialize,
    context: &'static str,
) -> Result<T, ApiError> {
    serde_json::from_value(serde_json::to_value(value).map_err(|e| internal_error(e, context))?)
        .map_err(|e| internal_error(e, context))
}

/// GET /api/tasks/{id}/artifacts
#[crate::instrument_api(method = "GET", path = "/api/tasks/{id}/artifacts")]
pub(crate) async fn get_task_artifacts(
    State(state): State<AppState>,
    Path(api_types::TaskIdParams { id: task_id }): Path<api_types::TaskIdParams>,
) -> Result<Json<api_types::ArtifactsResponse>, ApiError> {
    let artifacts = state
        .captain
        .task_artifacts(task_id)
        .await
        .map_err(|e| internal_error(e, "failed to load task artifacts"))?;
    Ok(Json(api_types::ArtifactsResponse {
        artifacts: wire(artifacts, "failed to serialize task artifacts")?,
    }))
}

/// GET /api/tasks/{id}/feed
///
/// Unified feed: merges timeline events, artifacts, and ask history into
/// a single chronologically-ordered stream.
#[crate::instrument_api(method = "GET", path = "/api/tasks/{id}/feed")]
pub(crate) async fn get_task_feed(
    State(state): State<AppState>,
    Path(api_types::TaskIdParams { id: task_id }): Path<api_types::TaskIdParams>,
) -> Result<Json<api_types::FeedResponse>, ApiError> {
    let id = task_id.to_string();
    let item = state
        .captain
        .load_task(task_id)
        .await
        .map_err(|e| internal_error(e, "failed to load task"))?;
    if item.is_none() {
        return Err(error_response(
            StatusCode::NOT_FOUND,
            &format!("task {task_id} not found"),
        ));
    }

    // Load all three data sources in parallel.
    let task_id_str = task_id.to_string();
    let (timeline_result, artifacts_result, history_result) = tokio::join!(
        state.captain.task_timeline(&task_id_str),
        state.captain.task_artifacts(task_id),
        state.captain.task_ask_history(task_id),
    );

    let timeline_events: Vec<api_types::TimelineEvent> = wire(
        timeline_result.map_err(|e| internal_error(e, "failed to load task timeline"))?,
        "failed to serialize task timeline",
    )?;
    let artifacts: Vec<api_types::TaskArtifact> = wire(
        artifacts_result.map_err(|e| internal_error(e, "failed to load task artifacts"))?,
        "failed to serialize task artifacts",
    )?;
    let history: Vec<api_types::AskHistoryEntry> = wire(
        history_result.map_err(|e| internal_error(e, "failed to load ask history"))?,
        "failed to serialize ask history",
    )?;

    // Build unified feed items with a type discriminator.
    let mut feed: Vec<api_types::FeedItem> = Vec::new();

    // Build lookup for labeling human messages as reopen/rework via ask_id.
    // Scope to `human_ask` events only and skip the default "ask" intent
    // (normal Q&A); only reopen/rework intents should surface as feed badges.
    let mut intent_by_ask: HashMap<String, String> = HashMap::new();
    for event in &timeline_events {
        let api_types::TimelineEventPayload::HumanAsk { intent, ask_id, .. } = &event.data else {
            continue;
        };
        if intent.is_empty() || intent == "ask" {
            continue;
        }
        intent_by_ask.insert(ask_id.clone(), intent.clone());
    }

    // Timeline events.
    for event in &timeline_events {
        feed.push(api_types::FeedItem::Timeline {
            timestamp: event.timestamp.clone(),
            data: event.clone(),
        });
    }

    // Artifacts.
    for artifact in artifacts {
        feed.push(api_types::FeedItem::Artifact {
            timestamp: artifact.created_at.clone(),
            data: artifact,
        });
    }

    // Ask history / advisor messages. Inject intent on human entries whose
    // ask_id matches a reopen/rework HumanAsk timeline event.
    for entry in history {
        let mut entry = entry;
        if entry.role == "human" {
            if let Some(intent) = intent_by_ask.get(&entry.ask_id).cloned() {
                entry.intent = Some(intent);
            }
        }
        feed.push(api_types::FeedItem::Message {
            timestamp: entry.timestamp.clone(),
            data: entry,
        });
    }

    // Sort by timestamp.
    feed.sort_by(|a, b| feed_timestamp(a).cmp(feed_timestamp(b)));

    Ok(Json(api_types::FeedResponse {
        id,
        count: feed.len(),
        feed,
    }))
}

/// GET /api/tasks/{id}/history
#[crate::instrument_api(method = "GET", path = "/api/tasks/{id}/history")]
pub(crate) async fn get_task_history(
    State(state): State<AppState>,
    Path(api_types::TaskIdParams { id: task_id }): Path<api_types::TaskIdParams>,
) -> Result<Json<api_types::AskHistoryResponse>, ApiError> {
    let entries = state
        .captain
        .task_ask_history(task_id)
        .await
        .map_err(|e| internal_error(e, "failed to load ask history"))?;

    Ok(Json(api_types::AskHistoryResponse {
        history: wire(entries, "failed to serialize ask history")?,
    }))
}

/// GET /api/tasks/{id}/timeline
#[crate::instrument_api(method = "GET", path = "/api/tasks/{id}/timeline")]
pub(crate) async fn get_task_timeline(
    State(state): State<AppState>,
    Path(api_types::TaskIdParams { id: task_id }): Path<api_types::TaskIdParams>,
) -> Result<Json<api_types::TimelineResponse>, ApiError> {
    let id = task_id.to_string();
    let events = state
        .captain
        .task_timeline(&id)
        .await
        .map_err(|e| internal_error(e, "failed to load task timeline"))?;

    let events: Vec<api_types::TimelineEvent> = wire(events, "failed to serialize task timeline")?;
    let count = events.len();
    Ok(Json(api_types::TimelineResponse { id, events, count }))
}

/// GET /api/tasks/{id}/sessions?caller=workers
#[crate::instrument_api(method = "GET", path = "/api/tasks/{id}/sessions")]
pub(crate) async fn get_task_sessions(
    State(state): State<AppState>,
    Path(api_types::TaskIdParams { id: id_num }): Path<api_types::TaskIdParams>,
    Query(params): Query<api_types::SessionsQuery>,
) -> Result<Json<api_types::ItemSessionsResponse>, ApiError> {
    let sessions = state
        .captain
        .list_task_sessions(id_num)
        .await
        .map_err(|e| internal_error(e, "failed to load task sessions"))?;

    let caller_filter = params.caller.as_deref().or(params.category.as_deref());

    let matched: Vec<api_types::SessionSummary> = sessions
        .into_iter()
        .filter(|e| match caller_filter {
            Some(filter) => {
                SessionCaller::parse(&e.caller).is_some_and(|c| c.group().as_str() == filter)
            }
            None => true,
        })
        .map(|e| {
            let status = serde_json::from_value::<api_types::SessionStatus>(
                serde_json::Value::String(e.status.clone()),
            )
            .map_err(|err| {
                internal_error(err, "failed to parse session status as SessionStatus enum")
            })?;
            Ok(api_types::SessionSummary {
                session_id: e.session_id,
                status,
                caller: e.caller,
                started_at: e.created_at,
                duration_ms: e.duration_ms,
                cost_usd: e.cost_usd,
                model: Some(e.model),
                resumed: e.resumed != 0,
                cwd: Some(e.cwd),
                worker_name: e.worker_name,
            })
        })
        .collect::<Result<_, ApiError>>()?;

    let count = matched.len();
    Ok(Json(api_types::ItemSessionsResponse {
        sessions: matched,
        count,
    }))
}

/// GET /api/tasks/{id}/pr-summary
#[crate::instrument_api(method = "GET", path = "/api/tasks/{id}/pr-summary")]
pub(crate) async fn get_task_pr_summary(
    State(state): State<AppState>,
    Path(api_types::TaskIdParams { id: id_num }): Path<api_types::TaskIdParams>,
) -> Result<Json<api_types::PrSummaryResponse>, ApiError> {
    let id = id_num.to_string();
    // Read store, extract what we need, then drop the guard before network I/O.
    let (pr_number, github_repo) = {
        let item = state
            .captain
            .load_task(id_num)
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
            match state.captain.fetch_pr_body(&repo, num).await {
                Ok(body) if !body.is_empty() => (Some(body), None),
                Ok(_) => (None, None),
                Err(e) => {
                    tracing::warn!(
                        module = "transport-http-transport-routes_task_detail", task_id = %id,
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

    Ok(Json(api_types::PrSummaryResponse {
        pr_number,
        summary,
        summary_error,
    }))
}
