//! AI-powered utility endpoints (todo parsing, etc.).

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use settings::io::projects as db_projects;

use crate::response::{error_response, internal_error};
use crate::AppState;

#[derive(Deserialize)]
pub struct ParseTodosRequest {
    pub text: String,
    pub project: String,
}

#[derive(Serialize)]
pub struct ParseTodosResponse {
    pub items: Vec<String>,
}

/// `POST /api/ai/parse-todos` -- parse free-form text into individual task titles.
///
/// All input (single-line or multi-line) goes through AI for title cleanup.
/// Constraint: number of returned items <= number of non-empty input lines.
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

    let wf = state.captain_workflow.load();
    let model = &wf.models.todo_parse;
    let timeout = wf.agent.todo_parse_timeout_s;
    let idle_ttl = wf.agent.todo_parse_idle_ttl_s;

    let pool = state.db.pool();
    let row = db_projects::resolve(pool, &body.project)
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
    let mgr = &state.cc_session_mgr;
    let session_key = format!("parse-todos-{}", global_infra::uuid::Uuid::v4().short());

    let result = mgr
        .start_with_item(
            &session_key,
            &prompt,
            &cwd,
            Some(model),
            idle_ttl,
            timeout,
            None,
            Some(max_turns),
        )
        .await
        .map_err(|e| internal_error(e, "todo parse session failed"))?;

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
