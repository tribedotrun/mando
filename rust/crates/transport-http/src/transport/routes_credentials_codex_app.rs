//! ChatGPT desktop-app account swap routes.
//!
//! The settings runtime owns credential-slot and macOS process semantics;
//! these handlers are typed transport adapters shared by Electron and CLI.

use std::path::PathBuf;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::Json;

use crate::response::{error_response, ApiError};
use crate::{ApiRouter, AppState};

pub(crate) fn codex_desktop_app_routes() -> ApiRouter<AppState> {
    let router = ApiRouter::new();
    let router = crate::api_route!(
        router,
        POST "/api/credentials/codex/app/use",
        transport = Json,
        auth = Protected,
        handler = use_codex_desktop_app,
        body = api_types::CodexDesktopAppUseRequest,
        res = api_types::CodexDesktopAppOperationResponse
    );
    let router = crate::api_route!(
        router,
        POST "/api/credentials/codex/app/restore",
        transport = Json,
        auth = Protected,
        handler = restore_codex_desktop_app,
        body = api_types::CodexDesktopAppRestoreRequest,
        res = api_types::CodexDesktopAppOperationResponse
    );
    crate::api_route!(
        router,
        GET "/api/credentials/codex/app/status",
        transport = Json,
        auth = Protected,
        handler = get_codex_desktop_app_status,
        query = api_types::CodexDesktopAppStatusQuery,
        res = api_types::CodexDesktopAppStatusResponse
    )
}

async fn use_codex_desktop_app(
    State(state): State<AppState>,
    Json(body): Json<api_types::CodexDesktopAppUseRequest>,
) -> Result<Json<api_types::CodexDesktopAppOperationResponse>, ApiError> {
    let label = body.label.trim();
    if label.is_empty() {
        return Err(error_response(StatusCode::BAD_REQUEST, "label is required"));
    }
    let codex_home = optional_path(body.codex_home);
    let caller_pid = body.caller_pid;
    match state
        .settings
        .use_codex_desktop_app(label, codex_home.as_deref(), caller_pid, &state.bus)
        .await
    {
        Ok(response) => Ok(Json(response)),
        Err(error) => Err(map_app_error(error)),
    }
}

async fn restore_codex_desktop_app(
    State(state): State<AppState>,
    Json(body): Json<api_types::CodexDesktopAppRestoreRequest>,
) -> Result<Json<api_types::CodexDesktopAppOperationResponse>, ApiError> {
    let codex_home = optional_path(body.codex_home);
    match state
        .settings
        .restore_codex_desktop_app(codex_home.as_deref(), &state.bus)
        .await
    {
        Ok(response) => Ok(Json(response)),
        Err(error) => Err(map_app_error(error)),
    }
}

async fn get_codex_desktop_app_status(
    State(state): State<AppState>,
    Query(query): Query<api_types::CodexDesktopAppStatusQuery>,
) -> Result<Json<api_types::CodexDesktopAppStatusResponse>, ApiError> {
    let codex_home = optional_path(query.codex_home);
    state
        .settings
        .codex_desktop_app_status(codex_home.as_deref())
        .map(Json)
        .map_err(map_app_error)
}

fn optional_path(value: Option<String>) -> Option<PathBuf> {
    value
        .map(|path| path.trim().to_string())
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
}

fn map_app_error(error: settings::CodexDesktopAppError) -> ApiError {
    let status = match error {
        settings::CodexDesktopAppError::NoUsableCredential(_) => StatusCode::NOT_FOUND,
        settings::CodexDesktopAppError::NotSwapped
        | settings::CodexDesktopAppError::NoPersonalStash => StatusCode::CONFLICT,
        settings::CodexDesktopAppError::Credential(_)
        | settings::CodexDesktopAppError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
    };
    let message = error.to_string();
    tracing::error!(
        module = "credentials-codex-desktop-app",
        %status,
        error = %message,
        "Codex desktop app operation failed"
    );
    error_response(status, &message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn optional_path_ignores_missing_and_blank_overrides() {
        assert!(optional_path(None).is_none());
        assert!(optional_path(Some("   ".into())).is_none());
        assert_eq!(
            optional_path(Some(" /tmp/codex-home ".into())),
            Some(PathBuf::from("/tmp/codex-home"))
        );
    }
}
