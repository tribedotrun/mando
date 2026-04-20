use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::Json;
use base64::Engine;
use futures_util::stream::Stream;
use std::convert::Infallible;

use crate::response::{error_response, ApiError};
use crate::{ApiRouter, AppState};
use terminal::{Agent, TerminalSize};

pub(crate) fn routes() -> ApiRouter<AppState> {
    let router = ApiRouter::new();
    let router = crate::api_route!(
        router,
        GET "/api/terminal",
        transport = Json,
        auth = Protected,
        handler = get_terminal_list,
        res = Vec<api_types::TerminalSessionInfo>
    );
    let router = crate::api_route!(
        router,
        POST "/api/terminal",
        transport = Json,
        auth = Protected,
        handler = post_terminal_create,
        body = api_types::TerminalCreateRequest,
        res = api_types::TerminalSessionInfo
    );
    let router = crate::api_route!(
        router,
        GET "/api/terminal/{id}",
        transport = Json,
        auth = Protected,
        handler = get_terminal_info,
        params = api_types::TerminalIdParams,
        res = api_types::TerminalSessionInfo
    );
    let router = crate::api_route!(
        router,
        DELETE "/api/terminal/{id}",
        transport = Json,
        auth = Protected,
        handler = delete_terminal,
        params = api_types::TerminalIdParams,
        res = api_types::BoolOkResponse
    );
    let router = crate::api_route!(
        router,
        POST "/api/terminal/{id}/write",
        transport = Json,
        auth = Protected,
        handler = post_terminal_write,
        body = api_types::TerminalWriteRequest,
        params = api_types::TerminalIdParams,
        res = api_types::BoolOkResponse
    );
    let router = crate::api_route!(
        router,
        POST "/api/terminal/{id}/resize",
        transport = Json,
        auth = Protected,
        handler = post_terminal_resize,
        body = api_types::TerminalSize,
        params = api_types::TerminalIdParams,
        res = api_types::BoolOkResponse
    );
    let router = crate::api_route!(
        router,
        GET "/api/terminal/{id}/stream",
        transport = Sse,
        auth = Protected,
        handler = get_terminal_stream,
        event = api_types::TerminalStreamEnvelope,
        query = api_types::TerminalStreamQuery,
        params = api_types::TerminalIdParams
    );
    let router = crate::api_route!(
        router,
        POST "/api/terminal/{id}/cc-session",
        transport = Json,
        auth = Protected,
        handler = post_terminal_cc_session,
        body = api_types::TerminalCcSessionRequest,
        params = api_types::TerminalIdParams,
        res = api_types::BoolOkResponse
    );
    crate::api_route!(
        router,
        POST "/api/terminal/{id}/activity",
        transport = Json,
        auth = Protected,
        handler = post_terminal_activity,
        body = api_types::EmptyRequest,
        params = api_types::TerminalIdParams,
        res = api_types::BoolTouchedResponse
    )
}

fn terminal_agent_from_wire(agent: api_types::TerminalAgent) -> Agent {
    match agent {
        api_types::TerminalAgent::Claude => Agent::Claude,
        api_types::TerminalAgent::Codex => Agent::Codex,
    }
}

fn terminal_size_from_wire(size: api_types::TerminalSize) -> TerminalSize {
    TerminalSize {
        rows: size.rows,
        cols: size.cols,
    }
}

fn terminal_info_from_session(
    session: impl serde::Serialize,
) -> Result<api_types::TerminalSessionInfo, ApiError> {
    serde_json::from_value(
        serde_json::to_value(session)
            .map_err(|e| error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))?,
    )
    .map_err(|e| error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()))
}

#[crate::instrument_api(method = "POST", path = "/api/terminal")]
pub(crate) async fn post_terminal_create(
    State(state): State<AppState>,
    Json(body): Json<api_types::TerminalCreateRequest>,
) -> Result<Json<api_types::TerminalSessionInfo>, ApiError> {
    let cwd: std::path::PathBuf = body.cwd.into();
    if !cwd.is_dir() {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "cwd must be an existing directory",
        ));
    }
    let project_name = body.project.clone();
    let cwd_str = cwd.to_string_lossy().to_string();
    let workbench_id = state
        .captain
        .prepare_terminal_workbench(&project_name, &cwd_str, body.resume_session_id.is_some())
        .await
        .map_err(|e| {
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("failed to prepare workbench: {e}"),
            )
        })?;
    let create_args = terminal::CreateTerminalArgs {
        project: body.project,
        cwd,
        agent: terminal_agent_from_wire(body.agent),
        resume_session_id: body.resume_session_id,
        size: body.size.map(terminal_size_from_wire),
        terminal_id: body.terminal_id,
        name: body.name,
    };
    let session = match state.terminal.create(create_args) {
        Ok(s) => s,
        Err(e) => {
            if let Some(id) = workbench_id {
                state.captain.rollback_terminal_workbench(id).await;
            }
            return Err(error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("failed to create terminal: {e}"),
            ));
        }
    };

    Ok(Json(terminal_info_from_session(session.info())?))
}

#[crate::instrument_api(method = "GET", path = "/api/terminal")]
pub(crate) async fn get_terminal_list(
    State(state): State<AppState>,
) -> Json<Vec<api_types::TerminalSessionInfo>> {
    let sessions = state
        .terminal
        .list()
        .into_iter()
        .filter_map(|session| serde_json::from_value(serde_json::to_value(session).ok()?).ok())
        .collect();
    Json(sessions)
}

#[crate::instrument_api(method = "POST", path = "/api/terminal/{id}/write")]
pub(crate) async fn post_terminal_write(
    State(state): State<AppState>,
    Path(api_types::TerminalIdParams { id }): Path<api_types::TerminalIdParams>,
    Json(body): Json<api_types::TerminalWriteRequest>,
) -> Result<Json<api_types::BoolOkResponse>, ApiError> {
    let session = state
        .terminal
        .session(&id)
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
    Ok(Json(api_types::BoolOkResponse { ok: true }))
}

#[crate::instrument_api(method = "POST", path = "/api/terminal/{id}/resize")]
pub(crate) async fn post_terminal_resize(
    State(state): State<AppState>,
    Path(api_types::TerminalIdParams { id }): Path<api_types::TerminalIdParams>,
    Json(size): Json<api_types::TerminalSize>,
) -> Result<Json<api_types::BoolOkResponse>, ApiError> {
    state
        .terminal
        .resize(&id, terminal_size_from_wire(size))
        .map_err(|e| {
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("resize failed: {e}"),
            )
        })?;
    Ok(Json(api_types::BoolOkResponse { ok: true }))
}

#[crate::instrument_api(method = "DELETE", path = "/api/terminal/{id}")]
pub(crate) async fn delete_terminal(
    State(state): State<AppState>,
    Path(api_types::TerminalIdParams { id }): Path<api_types::TerminalIdParams>,
) -> Result<Json<api_types::BoolOkResponse>, ApiError> {
    state.terminal.remove(&id);
    Ok(Json(api_types::BoolOkResponse { ok: true }))
}

#[crate::instrument_api(method = "GET", path = "/api/terminal/{id}/stream")]
pub(crate) async fn get_terminal_stream(
    State(state): State<AppState>,
    Path(api_types::TerminalIdParams { id }): Path<api_types::TerminalIdParams>,
    axum::extract::Query(query): axum::extract::Query<api_types::TerminalStreamQuery>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let session = state
        .terminal
        .session(&id)
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
            let payload = api_types::TerminalStreamEnvelope::Output(api_types::TerminalOutputPayload {
                data_b64: b64,
            });
            yield Ok(Event::default().data(match serde_json::to_string(&payload) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(target: "transport-http", module = "transport-http", %e, "failed to encode terminal stream payload");
                    return;
                }
            }));
        }

        if initial_state != terminal::SessionState::Live {
            let payload =
                api_types::TerminalStreamEnvelope::Exit(api_types::TerminalExitPayload {
                    code: initial_exit_code,
                });
            match serde_json::to_string(&payload) {
                Ok(s) => {
                    yield Ok(Event::default().data(s));
                }
                Err(e) => {
                    tracing::warn!(target: "transport-http", module = "transport-http", %e, "failed to encode terminal exit payload");
                }
            }
            return;
        }

        loop {
            match rx.recv().await {
                Ok(terminal::TerminalEvent::Output(data)) => {
                    let b64 = base64::engine::general_purpose::STANDARD.encode(&data);
                    let payload = api_types::TerminalStreamEnvelope::Output(
                        api_types::TerminalOutputPayload { data_b64: b64 },
                    );
                    let event = Event::default().data(match serde_json::to_string(&payload) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(target: "transport-http", module = "transport-http", %e, "failed to encode terminal stream payload");
                    continue;
                }
            });
                    yield Ok(event);
                }
                Ok(terminal::TerminalEvent::Exit { code }) => {
                    let payload =
                        api_types::TerminalStreamEnvelope::Exit(api_types::TerminalExitPayload {
                            code,
                        });
                    let event = Event::default().data(match serde_json::to_string(&payload) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(target: "transport-http", module = "transport-http", %e, "failed to encode terminal stream payload");
                    continue;
                }
            });
                    yield Ok(event);
                    break;
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::warn!(module = "transport-http-transport-routes_terminal", session = id, lagged = n, "terminal stream lagged");
                }
                Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
            }
        }
    };

    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

#[crate::instrument_api(method = "GET", path = "/api/terminal/{id}")]
pub(crate) async fn get_terminal_info(
    State(state): State<AppState>,
    Path(api_types::TerminalIdParams { id }): Path<api_types::TerminalIdParams>,
) -> Result<Json<api_types::TerminalSessionInfo>, ApiError> {
    let info = state
        .terminal
        .info(&id)
        .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "terminal session not found"))?;
    Ok(Json(terminal_info_from_session(info)?))
}

/// Callback endpoint hit by the Claude Code SessionStart hook. Records the
/// Claude conversation session id against the mando terminal session so a
/// future `--resume` can restore the conversation after a daemon restart.
#[crate::instrument_api(method = "POST", path = "/api/terminal/{id}/cc-session")]
pub(crate) async fn post_terminal_cc_session(
    State(state): State<AppState>,
    Path(api_types::TerminalIdParams { id }): Path<api_types::TerminalIdParams>,
    Json(body): Json<api_types::TerminalCcSessionRequest>,
) -> Result<Json<api_types::BoolOkResponse>, ApiError> {
    if body.cc_session_id.trim().is_empty() {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "cc_session_id must not be empty",
        ));
    }
    let session = state
        .terminal
        .session(&id)
        .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "terminal session not found"))?;
    let cwd = session.info().cwd.to_string_lossy().to_string();
    session
        .set_cc_session_id(body.cc_session_id.clone())
        .map_err(|e| {
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("failed to persist cc_session_id: {e}"),
            )
        })?;

    state
        .captain
        .record_terminal_cc_session(&cwd, &body.cc_session_id)
        .await
        .map_err(|e| {
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("failed to persist terminal workbench metadata: {e}"),
            )
        })?;

    Ok(Json(api_types::BoolOkResponse { ok: true }))
}

/// Callback endpoint hit by the Claude Code `UserPromptSubmit` hook, once
/// per user-submitted prompt. Looks up the workbench owning the terminal's
/// cwd and bumps its `last_activity_at` timestamp, broadcasting on the bus
/// so the sidebar can reorder immediately.
#[crate::instrument_api(method = "POST", path = "/api/terminal/{id}/activity")]
pub(crate) async fn post_terminal_activity(
    State(state): State<AppState>,
    Path(api_types::TerminalIdParams { id }): Path<api_types::TerminalIdParams>,
    Json(_body): Json<api_types::EmptyRequest>,
) -> Result<Json<api_types::BoolTouchedResponse>, ApiError> {
    let session = state
        .terminal
        .session(&id)
        .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "terminal session not found"))?;
    let cwd = session.info().cwd.to_string_lossy().to_string();
    let touched = state
        .captain
        .notify_terminal_activity(&cwd)
        .await
        .map_err(|e| {
            error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("touch_activity failed: {e}"),
            )
        })?;
    Ok(Json(api_types::BoolTouchedResponse { ok: true, touched }))
}
