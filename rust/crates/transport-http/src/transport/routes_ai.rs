//! AI-powered utility endpoints (todo parsing, etc.).

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use rustc_hash::FxHashMap;

use crate::response::{error_response, internal_error, ApiError};
use crate::AppState;

/// `POST /api/ai/parse-todos` -- parse free-form text into individual task titles.
///
/// All input (single-line or multi-line) goes through AI for title cleanup.
/// Constraint: number of returned items <= number of non-empty input lines.
pub async fn post_parse_todos(
    State(state): State<AppState>,
    Json(body): Json<api_types::ParseTodosRequest>,
) -> Result<Json<api_types::ParseTodosResponse>, ApiError> {
    let text = body.text.trim();
    if text.is_empty() {
        return Err(error_response(StatusCode::BAD_REQUEST, "text is empty"));
    }

    let line_count = text
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .count();

    let wf = state.settings.load_captain_workflow();
    let model = &wf.models.todo_parse;
    let timeout = wf.agent.todo_parse_timeout_s;
    let idle_ttl = wf.agent.todo_parse_idle_ttl_s;

    let row = state
        .settings
        .resolve_project(&body.project)
        .await
        .map_err(|e| internal_error(e, "failed to resolve project"))?
        .ok_or_else(|| {
            error_response(
                StatusCode::BAD_REQUEST,
                &format!("unknown project: {}", body.project),
            )
        })?;
    let cwd = global_infra::paths::expand_tilde(&row.path);
    if !cwd.is_dir() {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            &format!("project '{}' path is not a directory", row.name),
        ));
    }

    let mut vars: FxHashMap<&str, String> = FxHashMap::default();
    vars.insert("text", text.to_string());
    vars.insert("line_count", line_count.to_string());
    vars.insert("project", row.name);
    let prompt = settings::config::render_prompt("todo_parse", &wf.prompts, &vars)
        .map_err(|e| internal_error(e, "failed to render parse prompt"))?;

    let max_turns = wf.agent.todo_parse_max_turns;
    let sessions = state.sessions.clone();
    let session_key = format!("parse-todos-{}", global_infra::uuid::Uuid::v4().short());

    let result = sessions
        .start_with_item(::sessions::SessionStartRequest {
            key: session_key.clone(),
            prompt,
            cwd,
            model: Some(model.to_string()),
            idle_ttl,
            call_timeout: timeout,
            task_id: None,
            max_turns: Some(max_turns),
        })
        .await
        .map_err(|e| internal_error(e, "todo parse session failed"))?;

    sessions.close(&session_key);

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

    Ok(Json(api_types::ParseTodosResponse { items }))
}
