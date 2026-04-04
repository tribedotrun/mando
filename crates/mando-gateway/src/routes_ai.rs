//! AI-powered utility endpoints (todo parsing, etc.).

use std::time::Duration;

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::response::{error_response, internal_error};
use crate::AppState;

#[derive(Deserialize)]
pub struct ParseTodosRequest {
    pub text: String,
    pub project: Option<String>,
}

#[derive(Serialize)]
pub struct ParseTodosResponse {
    pub items: Vec<String>,
}

/// `POST /api/ai/parse-todos` — parse free-form text into individual task titles.
///
/// Constraint: number of returned items ≤ number of non-empty input lines.
/// AI may merge consecutive lines that describe a single task but never splits
/// a single line into multiple tasks.
pub async fn post_parse_todos(
    State(state): State<AppState>,
    Json(body): Json<ParseTodosRequest>,
) -> Result<Json<ParseTodosResponse>, (StatusCode, Json<Value>)> {
    let text = body.text.trim();
    if text.is_empty() {
        return Err(error_response(StatusCode::BAD_REQUEST, "text is empty"));
    }

    let line_count = text
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .count();

    // Fast path: single non-empty line → single task, no AI call.
    if line_count <= 1 {
        return Ok(Json(ParseTodosResponse {
            items: vec![text.to_string()],
        }));
    }

    let project_context = body
        .project
        .as_deref()
        .map(|p| format!("\n\nProject context: these tasks are for the \"{p}\" project.\n"))
        .unwrap_or_default();

    let prompt = format!(
        "Parse this text into individual task titles for a todo list.{project_context}\n\
         Rules:\n\
         - Each task must be a clear, concise one-line title\n\
         - You may merge consecutive lines that describe the same task\n\
         - Never split a single line into multiple tasks\n\
         - Return at most {line_count} items\n\
         - Return ONLY a JSON array of strings, no other text\n\n\
         Text:\n{text}"
    );

    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("/tmp"));

    let mut mgr = state.cc_session_mgr.write().await;
    let session_key = format!("parse-todos-{}", mando_uuid::Uuid::v4().short());

    let result = mgr
        .start(
            &session_key,
            &prompt,
            &cwd,
            Some("sonnet"),
            Duration::from_secs(60),
            Duration::from_secs(30),
        )
        .await
        .map_err(internal_error)?;

    mgr.close(&session_key);

    let raw_text = result.text.trim();

    // Strip markdown fence if the model wraps the JSON.
    let json_str = if raw_text.starts_with("```") {
        raw_text
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim()
    } else {
        raw_text
    };

    let mut items: Vec<String> = serde_json::from_str(json_str).map_err(|e| {
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("AI returned invalid JSON: {e}"),
        )
    })?;

    // Enforce constraint: never more items than input lines.
    items.truncate(line_count);
    items.retain(|s| !s.trim().is_empty());

    Ok(Json(ParseTodosResponse { items }))
}
