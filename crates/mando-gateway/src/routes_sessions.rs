//! /api/sessions/* route handlers.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::response::{error_response, internal_error};
use crate::AppState;

#[derive(Deserialize, Default)]
pub(crate) struct SessionsQuery {
    #[serde(default = "default_page")]
    pub page: u32,
    #[serde(default = "default_per_page")]
    pub per_page: u32,
    pub category: Option<String>,
}

fn default_page() -> u32 {
    1
}
fn default_per_page() -> u32 {
    50
}

/// GET /api/sessions?page=1&per_page=50&category=worker
pub(crate) async fn get_sessions(
    State(state): State<AppState>,
    Query(params): Query<SessionsQuery>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let config = state.config.load_full();
    let store = state.task_store.read().await;

    let page = params.page.max(1) as usize;
    let per_page = params.per_page.max(1) as usize;

    let (entries, total) = store
        .list_sessions(page, per_page, params.category.as_deref())
        .await
        .map_err(|e| {
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("session query failed: {e}"),
            )
        })?;

    let cat_counts = store.category_counts().await;

    let total_pages = if total == 0 {
        1
    } else {
        total.div_ceil(per_page)
    };

    let categories: Value = cat_counts
        .into_iter()
        .map(|(k, v)| (k, Value::Number(v.into())))
        .collect::<serde_json::Map<String, Value>>()
        .into();

    // Build task_id → title map for enrichment.
    let task_titles: std::collections::HashMap<String, String> = store
        .routing()
        .await
        .unwrap_or_default()
        .into_iter()
        .flat_map(|t| {
            let title = t.title.clone();
            let mut pairs = vec![(t.id.to_string(), title.clone())];
            if let Some(ref lid) = t.linear_id {
                pairs.push((lid.clone(), title));
            }
            pairs
        })
        .collect();

    // Build scout_item_id → title map for enrichment.
    let scout_ids: Vec<i64> = entries.iter().filter_map(|e| e.scout_item_id).collect();
    let scout_titles = mando_db::queries::scout::item_titles(store.pool(), &scout_ids)
        .await
        .unwrap_or_default();

    let page_entries: Vec<Value> = entries
        .iter()
        .map(|e| {
            let mut v = serde_json::to_value(e).unwrap_or(Value::Null);
            if let Value::Object(ref mut map) = v {
                let project = cwd_to_project(&e.cwd);
                let github_repo = crate::resolve_github_repo(project.as_deref(), &config);
                map.insert(
                    "github_repo".into(),
                    github_repo.map(Value::String).unwrap_or(Value::Null),
                );
                // Enrich with task title.
                if let Some(tid) = e.task_id.as_deref() {
                    if let Some(title) = task_titles.get(tid) {
                        map.insert("task_title".into(), Value::String(title.clone()));
                    }
                }
                // Enrich with scout item title.
                if let Some(sid) = e.scout_item_id {
                    if let Some(title) = scout_titles.get(&sid) {
                        map.insert("scout_item_title".into(), Value::String(title.clone()));
                    }
                }
            }
            v
        })
        .collect();

    let total_cost_usd = store.total_session_cost().await;

    Ok(Json(json!({
        "total": total,
        "page": page.min(total_pages),
        "per_page": per_page,
        "total_pages": total_pages,
        "categories": categories,
        "total_cost_usd": (total_cost_usd * 1000.0).round() / 1000.0,
        "sessions": page_entries,
    })))
}

/// Derive a project name from a CWD path (last path component).
fn cwd_to_project(cwd: &str) -> Option<String> {
    if cwd.is_empty() {
        return None;
    }
    std::path::Path::new(cwd)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
}

/// GET /api/sessions/{id}/transcript
pub(crate) async fn get_session_transcript(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let cache_dir = mando_config::state_dir().join("transcripts");

    // Markdown cache (completed sessions only) — serve immediately.
    let md_path = cache_dir.join(format!("{id}.md"));
    if let Ok(content) = tokio::fs::read_to_string(&md_path).await {
        return Ok(Json(json!({ "session_id": id, "markdown": content })));
    }

    // Read JSONL directly from CC's storage (deterministic path via CWD lookup).
    let cwd = {
        let store = state.task_store.read().await;
        store.session_cwd(&id).await
    };
    let jsonl = find_cc_transcript(&id, cwd.as_deref())
        .await
        .ok_or_else(|| {
            error_response(StatusCode::NOT_FOUND, &format!("transcript {id} not found"))
        })?;

    let markdown = mando_shared::transcript::jsonl_to_markdown(&jsonl);

    // Cache as .md only if the session is complete (not still running).
    if !is_session_running(&id) {
        if let Err(e) = tokio::fs::create_dir_all(&cache_dir).await {
            tracing::warn!(module = "sessions", error = %e, "failed to create transcript cache dir");
        } else if let Err(e) = tokio::fs::write(cache_dir.join(format!("{id}.md")), &markdown).await
        {
            tracing::warn!(module = "sessions", session_id = %id, error = %e, "failed to cache transcript");
        }
    }

    Ok(Json(json!({ "session_id": id, "markdown": markdown })))
}

/// Check if a session is still running by looking at stream meta.
fn is_session_running(session_id: &str) -> bool {
    let meta_path = mando_config::state_dir()
        .join("cc-streams")
        .join(format!("{session_id}.meta.json"));
    match std::fs::read_to_string(&meta_path) {
        Ok(content) => match serde_json::from_str::<serde_json::Value>(&content) {
            Ok(val) => val["status"].as_str() == Some("running"),
            Err(e) => {
                tracing::warn!(
                    module = "sessions",
                    session_id = %session_id,
                    error = %e,
                    "stream meta corrupt — treating as running to avoid caching incomplete transcript"
                );
                true
            }
        },
        Err(_) => false,
    }
}

/// Find a CC transcript by session ID.
async fn find_cc_transcript(session_id: &str, cwd: Option<&str>) -> Option<String> {
    let home = std::env::var("HOME").ok()?;
    let projects_dir = std::path::PathBuf::from(&home)
        .join(".claude")
        .join("projects");
    let target = format!("{session_id}.jsonl");

    // Try deterministic path via CWD from session DB or stream meta.
    let effective_cwd = cwd
        .map(String::from)
        .or_else(|| lookup_cwd_from_meta(session_id));
    if let Some(ref cwd_val) = effective_cwd {
        if !cwd_val.is_empty() {
            // CC sanitizes CWD by stripping the leading "/" then replacing "/" with "-".
            let sanitized = cwd_val.trim_start_matches('/').replace('/', "-");
            let path = projects_dir.join(&sanitized).join(&target);
            if let Ok(content) = tokio::fs::read_to_string(&path).await {
                return Some(content);
            }
        }
    }

    // Fallback: scan all project directories.
    let mut entries = tokio::fs::read_dir(&projects_dir).await.ok()?;
    while let Ok(Some(entry)) = entries.next_entry().await {
        let candidate = entry.path().join(&target);
        if let Ok(content) = tokio::fs::read_to_string(&candidate).await {
            return Some(content);
        }
    }
    None
}

// ── Structured transcript endpoints ──────────────────────────────────

#[derive(Deserialize, Default)]
pub(crate) struct MessagesQuery {
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

/// GET /api/sessions/{id}/messages?limit=N&offset=M
pub(crate) async fn get_session_messages(
    Path(id): Path<String>,
    Query(params): Query<MessagesQuery>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let stream = mando_config::stream_path_for_session(&id);
    if !stream.exists() {
        return Err(error_response(
            StatusCode::NOT_FOUND,
            &format!("stream not found for session {id}"),
        ));
    }
    let messages =
        mando_cc::transcript::parse_messages(&stream, params.limit, params.offset.unwrap_or(0));
    Ok(Json(
        serde_json::to_value(&messages).unwrap_or(Value::Array(vec![])),
    ))
}

/// GET /api/sessions/{id}/tools
pub(crate) async fn get_session_tools(
    Path(id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let stream = mando_config::stream_path_for_session(&id);
    if !stream.exists() {
        return Err(error_response(
            StatusCode::NOT_FOUND,
            &format!("stream not found for session {id}"),
        ));
    }
    let usage = mando_cc::transcript::tool_usage(&stream);
    Ok(Json(
        serde_json::to_value(&usage).unwrap_or(Value::Array(vec![])),
    ))
}

/// GET /api/sessions/{id}/cost
pub(crate) async fn get_session_cost(
    Path(id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let stream = mando_config::stream_path_for_session(&id);
    if !stream.exists() {
        return Err(error_response(
            StatusCode::NOT_FOUND,
            &format!("stream not found for session {id}"),
        ));
    }
    let cost = mando_cc::transcript::session_cost(&stream);
    let val = serde_json::to_value(&cost).map_err(internal_error)?;
    Ok(Json(val))
}

/// Look up the CWD from stream meta sidecar (fallback when DB has no entry).
fn lookup_cwd_from_meta(session_id: &str) -> Option<String> {
    let meta_path = mando_config::state_dir()
        .join("cc-streams")
        .join(format!("{session_id}.meta.json"));
    let content = std::fs::read_to_string(&meta_path).ok()?;
    let val: serde_json::Value = serde_json::from_str(&content).ok()?;
    val["cwd"]
        .as_str()
        .filter(|s| !s.is_empty())
        .map(String::from)
}
