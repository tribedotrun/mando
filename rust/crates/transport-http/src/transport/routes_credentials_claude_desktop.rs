//! Typed controls for per-account Desktop profiles and shared local Code sessions.
use crate::response::{error_response, ApiError};
use crate::{ApiRouter, AppState};
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::Json;

pub(crate) fn claude_desktop_routes() -> ApiRouter<AppState> {
    let router = ApiRouter::new();
    let router = crate::api_route!(
        router,
        GET "/api/credentials/claude/desktop/status",
        transport = Json,
        auth = Protected,
        handler = status,
        query = api_types::ClaudeDesktopProfileRequest,
        res = api_types::ClaudeDesktopProfileStatus
    );
    let router = crate::api_route!(
        router,
        POST "/api/credentials/claude/desktop/setup",
        transport = Json,
        auth = Protected,
        handler = setup,
        body = api_types::ClaudeDesktopProfileRequest,
        res = api_types::ClaudeDesktopProfileStatus
    );
    let router = crate::api_route!(
        router,
        POST "/api/credentials/claude/desktop/open",
        transport = Json,
        auth = Protected,
        handler = open,
        body = api_types::ClaudeDesktopProfileRequest,
        res = api_types::ClaudeDesktopProfileStatus
    );
    let router = crate::api_route!(
        router,
        POST "/api/credentials/claude/desktop/adopt",
        transport = Json,
        auth = Protected,
        handler = adopt,
        body = api_types::ClaudeDesktopProfileAdoptRequest,
        res = api_types::ClaudeDesktopProfileStatus
    );
    let router = crate::api_route!(
        router,
        POST "/api/credentials/claude/desktop/preview-sessions",
        transport = Json,
        auth = Protected,
        handler = preview_sessions,
        body = api_types::ClaudeDesktopSessionImportRequest,
        res = api_types::ClaudeDesktopSessionSyncResponse
    );
    crate::api_route!(
        router,
        POST "/api/credentials/claude/desktop/sync-sessions",
        transport = Json,
        auth = Protected,
        handler = sync_sessions,
        body = api_types::ClaudeDesktopSessionImportRequest,
        res = api_types::ClaudeDesktopSessionSyncResponse
    )
}

async fn status(
    State(state): State<AppState>,
    Query(body): Query<api_types::ClaudeDesktopProfileRequest>,
) -> Result<Json<api_types::ClaudeDesktopProfileStatus>, ApiError> {
    state
        .settings
        .claude_desktop_profile_status(body.credential_id)
        .await
        .map(Json)
        .map_err(map_error)
}
async fn setup(
    State(state): State<AppState>,
    Json(body): Json<api_types::ClaudeDesktopProfileRequest>,
) -> Result<Json<api_types::ClaudeDesktopProfileStatus>, ApiError> {
    state
        .settings
        .setup_claude_desktop_profile(body.credential_id)
        .await
        .map(Json)
        .map_err(map_error)
}
async fn open(
    State(state): State<AppState>,
    Json(body): Json<api_types::ClaudeDesktopProfileRequest>,
) -> Result<Json<api_types::ClaudeDesktopProfileStatus>, ApiError> {
    state
        .settings
        .open_claude_desktop_profile(body.credential_id)
        .await
        .map(Json)
        .map_err(map_error)
}
async fn adopt(
    State(state): State<AppState>,
    Json(body): Json<api_types::ClaudeDesktopProfileAdoptRequest>,
) -> Result<Json<api_types::ClaudeDesktopProfileStatus>, ApiError> {
    state
        .settings
        .adopt_claude_desktop_profile(
            body.credential_id,
            std::path::Path::new(&body.user_data_dir),
        )
        .await
        .map(Json)
        .map_err(map_error)
}
async fn sync_sessions(
    State(state): State<AppState>,
    Json(body): Json<api_types::ClaudeDesktopSessionImportRequest>,
) -> Result<Json<api_types::ClaudeDesktopSessionSyncResponse>, ApiError> {
    state
        .settings
        .sync_claude_desktop_sessions(body.credential_id, body.range, false)
        .await
        .map(Json)
        .map_err(map_error)
}
async fn preview_sessions(
    State(state): State<AppState>,
    Json(body): Json<api_types::ClaudeDesktopSessionImportRequest>,
) -> Result<Json<api_types::ClaudeDesktopSessionSyncResponse>, ApiError> {
    state
        .settings
        .sync_claude_desktop_sessions(body.credential_id, body.range, true)
        .await
        .map(Json)
        .map_err(map_error)
}
fn map_error(error: settings::ClaudeDesktopProfileError) -> ApiError {
    let status = match &error {
        settings::ClaudeDesktopProfileError::NotFound(_) => StatusCode::NOT_FOUND,
        settings::ClaudeDesktopProfileError::Conflict(_) => StatusCode::CONFLICT,
        settings::ClaudeDesktopProfileError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };
    tracing::error!(module = "claude-desktop", %error, %status, "Claude Desktop profile operation failed");
    error_response(status, &error.to_string())
}
