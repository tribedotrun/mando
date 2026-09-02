//! /api/sessions/* route handlers.

use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::Json;
use base64::Engine;
use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::response::{error_response, internal_error, ApiError};
use crate::AppState;

/// GET /api/sessions?page=1&per_page=50&category=worker
pub(crate) async fn get_sessions(
    State(state): State<AppState>,
    Query(params): Query<api_types::SessionsQuery>,
) -> Result<Json<api_types::SessionsListResponse>, ApiError> {
    let config = state.settings.load_config();
    let listing = state
        .sessions
        .list_sessions(sessions::SessionListRequest {
            page: params.page,
            per_page: params.per_page,
            category: params
                .category
                .map(|category| category.as_str().to_string()),
            caller: params.caller.map(|caller| caller.as_str().to_string()),
            status: params.status.map(|status| status.as_str().to_string()),
        })
        .await
        .map_err(|e| internal_error(e, "session query failed"))?;

    let sessions = listing
        .sessions
        .into_iter()
        .map(|mut entry| {
            let project = cwd_to_project(&entry.cwd);
            entry.github_repo = crate::resolve_github_repo(project.as_deref(), &config);
            entry
        })
        .collect();

    Ok(Json(api_types::SessionsListResponse {
        total: listing.total,
        page: listing.page,
        per_page: listing.per_page,
        total_pages: listing.total_pages,
        categories: listing.categories,
        total_cost_usd: listing.total_cost_usd,
        sessions,
    }))
}

/// GET /api/sessions/{id}
pub(crate) async fn get_session(
    State(state): State<AppState>,
    Path(api_types::SessionIdParams { id }): Path<api_types::SessionIdParams>,
) -> Result<Json<api_types::SessionEntry>, ApiError> {
    let config = state.settings.load_config();
    let mut entry = state
        .sessions
        .session_by_id(&id)
        .await
        .map_err(|e| internal_error(e, "session lookup failed"))?
        .ok_or_else(|| error_response(StatusCode::NOT_FOUND, &format!("session {id} not found")))?;
    let project = cwd_to_project(&entry.cwd);
    entry.github_repo = crate::resolve_github_repo(project.as_deref(), &config);
    Ok(Json(entry))
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

/// GET /api/sessions/{id}/events
///
/// Snapshot of every typed transcript event currently on disk for this
/// session. Used by the CLI and by the renderer's first-paint load; the
/// live-tail `/events/stream` route consumes the same underlying parser.
pub(crate) async fn get_session_events(
    State(state): State<AppState>,
    Path(api_types::SessionIdParams { id }): Path<api_types::SessionIdParams>,
) -> Result<Json<api_types::TranscriptEventsResponse>, ApiError> {
    let snapshot = state
        .sessions
        .events_snapshot(&id)
        .await
        .map_err(|e| internal_error(e, "failed to load session events"))?
        .ok_or_else(|| {
            error_response(
                StatusCode::NOT_FOUND,
                &format!("session {id} events not found"),
            )
        })?;

    Ok(Json(api_types::TranscriptEventsResponse {
        session_id: id,
        events: snapshot.events,
        is_running: snapshot.is_running,
    }))
}

/// GET /api/sessions/{id}/images/{tool}/{index}
///
/// Serves one image embedded in a Claude Code tool result. Raw base64 stays
/// out of the transcript snapshot and crosses the wire only when an image is
/// actually visible in the transcript UI.
pub(crate) async fn get_session_tool_result_image(
    State(state): State<AppState>,
    Path(api_types::SessionToolResultImageParams { id, tool, index }): Path<
        api_types::SessionToolResultImageParams,
    >,
) -> Result<axum::response::Response<Body>, ApiError> {
    let image = state
        .sessions
        .tool_result_image(&id, &tool, index as usize)
        .await
        .map_err(|e| internal_error(e, "failed to load session tool-result image"))?
        .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "tool-result image not found"))?;

    let content_type = match image.media_type.as_str() {
        "image/png" => "image/png",
        "image/jpeg" => "image/jpeg",
        "image/gif" => "image/gif",
        "image/webp" => "image/webp",
        _ => {
            return Err(error_response(
                StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "unsupported tool-result image type",
            ));
        }
    };
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(image.base64_data)
        .map_err(|e| internal_error(e, "invalid base64 in session tool-result image"))?;

    axum::response::Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, content_type)
        .header(header::CACHE_CONTROL, "private, max-age=3600")
        .body(Body::from(bytes))
        .map_err(|e| internal_error(e, "failed to build session tool-result image response"))
}

/// GET /api/sessions/{id}/jsonl-path
///
/// Resolves the on-disk path of the session's underlying JSONL stream so the
/// renderer can open it with the user's default app for `.jsonl`. Codex
/// sessions prefer Mando-owned app-server logs under `state/session-jsonl`,
/// while Claude Code sessions prefer `cc-streams` with the CC-native fallback.
/// Returns `{ path: null }` when no file exists so the UI can disable the
/// action.
pub(crate) async fn get_session_jsonl_path(
    State(state): State<AppState>,
    Path(api_types::SessionIdParams { id }): Path<api_types::SessionIdParams>,
) -> Result<Json<api_types::SessionJsonlPathResponse>, ApiError> {
    let path = state
        .sessions
        .session_jsonl_path(&id)
        .await
        .map_err(|e| internal_error(e, "failed to resolve session jsonl path"))?;

    Ok(Json(api_types::SessionJsonlPathResponse {
        session_id: id,
        path,
    }))
}

/// GET /api/sessions/{id}/messages?limit=N&offset=M
pub(crate) async fn get_session_messages(
    State(state): State<AppState>,
    Path(api_types::SessionIdParams { id }): Path<api_types::SessionIdParams>,
    Query(params): Query<api_types::SessionMessagesQuery>,
) -> Result<Json<api_types::SessionMessagesResponse>, ApiError> {
    let messages = state
        .sessions
        .session_messages(&id, params.limit, params.offset.unwrap_or(0))
        .await
        .map_err(|e| internal_error(e, "failed to load session messages"))?
        .ok_or_else(|| {
            error_response(
                StatusCode::NOT_FOUND,
                &format!("stream not found for session {id}"),
            )
        })?;

    Ok(Json(api_types::SessionMessagesResponse {
        messages: roundtrip(messages, "session messages")?,
    }))
}

/// GET /api/sessions/{id}/tools
pub(crate) async fn get_session_tools(
    State(state): State<AppState>,
    Path(api_types::SessionIdParams { id }): Path<api_types::SessionIdParams>,
) -> Result<Json<api_types::SessionToolUsageResponse>, ApiError> {
    let tools = state
        .sessions
        .session_tool_usage(&id)
        .await
        .map_err(|e| internal_error(e, "failed to load session tool usage"))?
        .ok_or_else(|| {
            error_response(
                StatusCode::NOT_FOUND,
                &format!("stream not found for session {id}"),
            )
        })?;

    Ok(Json(api_types::SessionToolUsageResponse {
        tools: roundtrip(tools, "session tool usage")?,
    }))
}

/// GET /api/sessions/{id}/cost
pub(crate) async fn get_session_cost(
    State(state): State<AppState>,
    Path(api_types::SessionIdParams { id }): Path<api_types::SessionIdParams>,
) -> Result<Json<api_types::SessionCostResponse>, ApiError> {
    let cost = state
        .sessions
        .session_cost(&id)
        .await
        .map_err(|e| internal_error(e, "failed to load session cost"))?
        .ok_or_else(|| {
            error_response(
                StatusCode::NOT_FOUND,
                &format!("stream not found for session {id}"),
            )
        })?;

    Ok(Json(api_types::SessionCostResponse {
        cost: session_cost_summary(cost),
    }))
}

fn session_cost_summary(cost: global_claude::SessionCost) -> api_types::SessionCostSummary {
    let global_claude::SessionCost {
        total_input_tokens,
        total_output_tokens,
        total_cache_read_tokens,
        total_cache_creation_tokens,
        turn_count,
        total_cost_usd,
        per_model_usage: _,
    } = cost;

    api_types::SessionCostSummary {
        total_input_tokens,
        total_output_tokens,
        total_cache_read_tokens,
        total_cache_creation_tokens,
        turn_count,
        total_cost_usd,
    }
}

/// GET /api/sessions/{id}/stream?types=user,assistant,result
///
/// Returns the raw JSONL stream for a session as newline-delimited JSON.
/// When `types` is supplied only lines whose `"type"` field matches are included.
pub(crate) async fn get_session_stream(
    State(state): State<AppState>,
    Path(api_types::SessionIdParams { id }): Path<api_types::SessionIdParams>,
    Query(params): Query<api_types::SessionStreamQuery>,
) -> Result<axum::response::Response, ApiError> {
    let type_filter = params.types.as_deref().map(|items| {
        items
            .split(',')
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect::<Vec<_>>()
    });

    let content = state
        .sessions
        .session_stream(&id, type_filter)
        .await
        .map_err(|e| internal_error(e, "failed to load session stream"))?
        .ok_or_else(|| {
            error_response(
                StatusCode::NOT_FOUND,
                &format!("stream not found for session {id}"),
            )
        })?;

    match axum::response::Response::builder()
        .status(StatusCode::OK)
        .header(axum::http::header::CONTENT_TYPE, "application/x-ndjson")
        .body(axum::body::Body::from(content))
    {
        Ok(resp) => Ok(resp),
        Err(e) => Err(error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("failed to build session stream response: {e}"),
        )),
    }
}

fn roundtrip<T: DeserializeOwned>(
    value: impl Serialize,
    label: &'static str,
) -> Result<T, ApiError> {
    serde_json::from_value(
        serde_json::to_value(value)
            .map_err(|e| internal_error(e, &format!("failed to serialize {label}")))?,
    )
    .map_err(|e| internal_error(e, &format!("failed to decode {label}")))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn populated_session_cost_serializes_to_wire_response() {
        let cost = global_claude::SessionCost {
            total_input_tokens: 120,
            total_output_tokens: 34,
            total_cache_read_tokens: 56,
            total_cache_creation_tokens: 78,
            turn_count: 2,
            total_cost_usd: Some(0.42),
            per_model_usage: HashMap::from([(
                "claude-sonnet-4-6".to_string(),
                global_claude::ModelUsage {
                    input_tokens: 120,
                    output_tokens: 34,
                    cache_read_tokens: 56,
                    cache_creation_tokens: 78,
                },
            )]),
        };

        let response = api_types::SessionCostResponse {
            cost: session_cost_summary(cost),
        };
        let serialized = serde_json::to_value(response);
        assert!(serialized.is_ok(), "populated session cost must serialize");
        let Ok(serialized) = serialized else {
            return;
        };
        assert_eq!(
            serialized,
            serde_json::json!({
                "cost": {
                    "total_input_tokens": 120,
                    "total_output_tokens": 34,
                    "total_cache_read_tokens": 56,
                    "total_cache_creation_tokens": 78,
                    "turn_count": 2,
                    "total_cost_usd": 0.42,
                }
            })
        );
    }
}
