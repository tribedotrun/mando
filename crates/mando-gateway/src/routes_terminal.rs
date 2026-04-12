use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::{get, post};
use axum::Json;
use axum::Router;
use base64::Engine;
use futures_util::stream::Stream;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::convert::Infallible;

use crate::response::error_response;
use crate::AppState;
use mando_terminal::types::{Agent, TerminalSize};

pub(crate) fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/terminal",
            get(get_terminal_list).post(post_terminal_create),
        )
        .route(
            "/api/terminal/{id}",
            get(get_terminal_info).delete(delete_terminal),
        )
        .route("/api/terminal/{id}/write", post(post_terminal_write))
        .route("/api/terminal/{id}/resize", post(post_terminal_resize))
        .route("/api/terminal/{id}/stream", get(get_terminal_stream))
        .route(
            "/api/terminal/{id}/cc-session",
            post(post_terminal_cc_session),
        )
}

#[derive(Deserialize)]
pub(crate) struct CreateBody {
    pub project: String,
    pub cwd: String,
    pub agent: Agent,
    #[serde(default)]
    pub resume_session_id: Option<String>,
    #[serde(default)]
    pub size: Option<TerminalSize>,
    #[serde(default, alias = "terminalId")]
    pub terminal_id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
}

pub(crate) async fn post_terminal_create(
    State(state): State<AppState>,
    Json(body): Json<CreateBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let cwd: std::path::PathBuf = body.cwd.into();
    if !cwd.is_dir() {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "cwd must be an existing directory",
        ));
    }

    let mut terminal_env = HashMap::new();
    terminal_env.insert("MANDO_PORT".to_string(), state.listen_port.to_string());
    let auth_token = crate::auth::ensure_auth_token();
    terminal_env.insert("MANDO_AUTH_TOKEN".to_string(), auth_token);

    let cfg = state.config.load();
    let args_str = match &body.agent {
        Agent::Claude => cfg.captain.claude_terminal_args.clone(),
        Agent::Codex => cfg.captain.codex_terminal_args.clone(),
    };
    let config_env = cfg.env.clone();
    drop(cfg);

    let extra_args = shell_words::split(&args_str).map_err(|e| {
        error_response(
            StatusCode::BAD_REQUEST,
            &format!("malformed terminal args: {e}"),
        )
    })?;

    let project_name = body.project.clone();
    let cwd_str = cwd.to_string_lossy().to_string();

    let project_id = match mando_db::queries::projects::find_by_name(state.db.pool(), &project_name)
        .await
    {
        Ok(Some(row)) => row.id,
        Ok(None) => mando_db::queries::projects::upsert(state.db.pool(), &project_name, "", None)
            .await
            .map_err(|e| {
                error_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    &format!("failed to create project: {e}"),
                )
            })?,
        Err(e) => {
            return Err(error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("project lookup failed: {e}"),
            ));
        }
    };

    let wb_id = if body.resume_session_id.is_none() {
        let existing = mando_db::queries::workbenches::find_by_worktree(state.db.pool(), &cwd_str)
            .await
            .ok()
            .flatten();
        if existing.is_some() {
            None
        } else {
            let title = mando_types::workbench::workbench_title_now();
            let wb = mando_types::Workbench::new(project_id, project_name, cwd_str, title);
            Some(
                mando_db::queries::workbenches::insert(state.db.pool(), &wb)
                    .await
                    .map_err(|e| {
                        error_response(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            &format!("failed to create workbench: {e}"),
                        )
                    })?,
            )
        }
    } else {
        None
    };

    let req = mando_terminal::CreateRequest {
        project: body.project,
        cwd,
        agent: body.agent,
        resume_session_id: body.resume_session_id,
        size: body.size,
        config_env,
        terminal_env,
        terminal_id: body.terminal_id,
        extra_args,
        name: body.name,
    };
    let session = match state.terminal_host.create(req) {
        Ok(s) => s,
        Err(e) => {
            if let Some(id) = wb_id {
                let _ = mando_db::queries::workbenches::archive(state.db.pool(), id).await;
            }
            return Err(error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("failed to create terminal: {e}"),
            ));
        }
    };

    Ok(Json(json!(session.info())))
}

pub(crate) async fn get_terminal_list(State(state): State<AppState>) -> Json<Value> {
    Json(json!(state.terminal_host.list()))
}

#[derive(Deserialize)]
pub(crate) struct WriteBody {
    pub data: String,
}

pub(crate) async fn post_terminal_write(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<WriteBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let session = state
        .terminal_host
        .get(&id)
        .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "terminal session not found"))?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&body.data)
        .map_err(|e| error_response(StatusCode::BAD_REQUEST, &format!("invalid base64: {e}")))?;
    session.write_input(&bytes).await.map_err(|e| {
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("write failed: {e}"),
        )
    })?;
    Ok(Json(json!({"ok": true})))
}

pub(crate) async fn post_terminal_resize(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(size): Json<TerminalSize>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    state.terminal_host.resize(&id, size).map_err(|e| {
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("resize failed: {e}"),
        )
    })?;
    Ok(Json(json!({"ok": true})))
}

pub(crate) async fn delete_terminal(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    state.terminal_host.remove(&id);
    Ok(Json(json!({"ok": true})))
}

#[derive(Deserialize, Default)]
pub(crate) struct StreamQuery {
    #[serde(default)]
    pub replay: Option<u8>,
}

pub(crate) async fn get_terminal_stream(
    State(state): State<AppState>,
    Path(id): Path<String>,
    axum::extract::Query(query): axum::extract::Query<StreamQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, Json<Value>)> {
    let session = state
        .terminal_host
        .get(&id)
        .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "terminal session not found"))?;
    let mut rx = session.subscribe();
    let replay = query.replay.unwrap_or(1) != 0;
    let snapshot = if replay {
        session.snapshot()
    } else {
        Vec::new()
    };
    let initial_state = session.state();
    let initial_exit_code = session.exit_code();

    let stream = async_stream::stream! {
        if !snapshot.is_empty() {
            let b64 = base64::engine::general_purpose::STANDARD.encode(&snapshot);
            yield Ok(Event::default().event("output").data(b64));
        }

        if initial_state != mando_terminal::SessionState::Live {
            let event = Event::default()
                .event("exit")
                .data(json!({"code": initial_exit_code}).to_string());
            yield Ok(event);
            return;
        }

        loop {
            match rx.recv().await {
                Ok(mando_terminal::TerminalEvent::Output(data)) => {
                    let b64 = base64::engine::general_purpose::STANDARD.encode(&data);
                    let event = Event::default().event("output").data(b64);
                    yield Ok(event);
                }
                Ok(mando_terminal::TerminalEvent::Exit { code }) => {
                    let event = Event::default()
                        .event("exit")
                        .data(json!({"code": code}).to_string());
                    yield Ok(event);
                    break;
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(session = id, lagged = n, "terminal stream lagged");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    };

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

pub(crate) async fn get_terminal_info(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    let session = state
        .terminal_host
        .get(&id)
        .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "terminal session not found"))?;
    Ok(Json(json!(session.info())))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CcSessionBody {
    pub cc_session_id: String,
}

/// Callback endpoint hit by the Claude Code SessionStart hook. Records the
/// Claude conversation session id against the mando terminal session so a
/// future `--resume` can restore the conversation after a daemon restart.
pub(crate) async fn post_terminal_cc_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<CcSessionBody>,
) -> Result<Json<Value>, (StatusCode, Json<Value>)> {
    if body.cc_session_id.trim().is_empty() {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "cc_session_id must not be empty",
        ));
    }
    let session = state
        .terminal_host
        .get(&id)
        .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "terminal session not found"))?;
    session.set_cc_session_id(body.cc_session_id).map_err(|e| {
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("failed to persist cc_session_id: {e}"),
        )
    })?;
    Ok(Json(json!({"ok": true})))
}
