use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::Json;
use base64::Engine;
use futures_util::stream::Stream;
use serde::Deserialize;
use serde_json::{json, Value};
use std::convert::Infallible;

use axum::routing::{get, post};
use axum::Router;

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
    let req = mando_terminal::CreateRequest {
        project: body.project,
        cwd,
        agent: body.agent,
        resume_session_id: body.resume_session_id,
        size: body.size,
    };
    let session = state.terminal_host.create(req).map_err(|e| {
        error_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("failed to create terminal: {e}"),
        )
    })?;
    Ok(Json(json!(session.info())))
}

pub(crate) async fn get_terminal_list(State(state): State<AppState>) -> Json<Value> {
    Json(json!(state.terminal_host.list()))
}

#[derive(Deserialize)]
pub(crate) struct WriteBody {
    pub data: String, // base64-encoded
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
    session.write_input(&bytes).map_err(|e| {
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

pub(crate) async fn get_terminal_stream(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, Json<Value>)> {
    let session = state
        .terminal_host
        .get(&id)
        .ok_or_else(|| error_response(StatusCode::NOT_FOUND, "terminal session not found"))?;
    let mut rx = session.subscribe();
    let snapshot = session.snapshot();

    let stream = async_stream::stream! {
        // Replay buffered output so late subscribers see startup content.
        if !snapshot.is_empty() {
            let b64 = base64::engine::general_purpose::STANDARD.encode(&snapshot);
            yield Ok(Event::default().event("output").data(b64));
        }

        loop {
            match rx.recv().await {
                Ok(mando_terminal::TerminalEvent::Output(data)) => {
                    let b64 = base64::engine::general_purpose::STANDARD.encode(&data);
                    let event = Event::default()
                        .event("output")
                        .data(b64);
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
                Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                    break;
                }
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
