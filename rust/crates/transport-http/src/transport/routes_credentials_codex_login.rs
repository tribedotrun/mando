//! Routes for the 1-click Codex "sign in with browser" flow: start, poll
//! status, cancel. Split out from `routes_credentials_codex.rs` to keep
//! that file under the file-length budget.

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use settings::CodexCredentialError;

use crate::response::{error_response, internal_error, ApiError};
use crate::{ApiRouter, AppState};

pub(crate) fn codex_login_routes() -> ApiRouter<AppState> {
    let router = ApiRouter::new();
    let router = crate::api_route!(
        router,
        POST "/api/credentials/codex/login/start",
        transport = Json,
        auth = Protected,
        handler = start_codex_login,
        body = api_types::StartCodexLoginRequest,
        res = api_types::StartCodexLoginResponse
    );
    let router = crate::api_route!(
        router,
        GET "/api/credentials/codex/login/current",
        transport = Json,
        auth = Protected,
        handler = get_codex_login_status,
        res = api_types::CodexLoginStatusResponse
    );
    crate::api_route!(
        router,
        POST "/api/credentials/codex/login/cancel",
        transport = Json,
        auth = Protected,
        handler = cancel_codex_login,
        body = api_types::EmptyRequest,
        res = api_types::CancelCodexLoginResponse
    )
}

/// POST /api/credentials/codex/login/start — begin a browser OAuth login.
/// Cancels and replaces any already-pending flow, then returns immediately;
/// the caller polls `GET .../login/current` for progress. When
/// `credential_id` is set (row-scoped re-login), the target row is
/// validated before anything spawns.
#[crate::instrument_api(method = "POST", path = "/api/credentials/codex/login/start")]
async fn start_codex_login(
    State(state): State<AppState>,
    Json(body): Json<api_types::StartCodexLoginRequest>,
) -> Result<Json<api_types::StartCodexLoginResponse>, ApiError> {
    match state
        .settings
        .start_codex_login(
            body.label,
            body.credential_id,
            state.bus.clone(),
            &state.task_tracker,
        )
        .await
    {
        Ok(started) => Ok(Json(api_types::StartCodexLoginResponse {
            ok: true,
            login_id: started.login_id,
        })),
        Err(CodexCredentialError::NotFound(id)) => Err(error_response(
            StatusCode::NOT_FOUND,
            &format!("credential id={id} not found"),
        )),
        Err(CodexCredentialError::NotCodex) => Err(error_response(
            StatusCode::BAD_REQUEST,
            "credential is not a Codex row",
        )),
        Err(CodexCredentialError::NoAccountId) => Err(error_response(
            StatusCode::BAD_REQUEST,
            "Codex credential has no account_id",
        )),
        Err(e) => Err(internal_error(
            anyhow::Error::msg(e.to_string()),
            "failed to start codex login",
        )),
    }
}

/// GET /api/credentials/codex/login/current — snapshot of the single
/// in-flight (or most-recently-finished) login flow. `flow` is `None` when
/// no flow has run since the daemon started.
#[crate::instrument_api(method = "GET", path = "/api/credentials/codex/login/current")]
async fn get_codex_login_status(
    State(state): State<AppState>,
) -> Json<api_types::CodexLoginStatusResponse> {
    let flow = state.settings.codex_login_status().await;
    Json(api_types::CodexLoginStatusResponse { flow })
}

/// POST /api/credentials/codex/login/cancel — cancel the pending flow, if
/// any. `cancelled` is `false` when there was nothing pending.
#[crate::instrument_api(method = "POST", path = "/api/credentials/codex/login/cancel")]
async fn cancel_codex_login(
    State(state): State<AppState>,
    Json(_body): Json<api_types::EmptyRequest>,
) -> Json<api_types::CancelCodexLoginResponse> {
    let cancelled = state.settings.cancel_codex_login().await;
    Json(api_types::CancelCodexLoginResponse {
        ok: true,
        cancelled,
    })
}
