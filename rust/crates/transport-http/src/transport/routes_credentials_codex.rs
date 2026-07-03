//! Codex-specific credential routes — POST add, POST pick.
//!
//! List / probe / delete paths in `routes_credentials.rs` handle Codex rows
//! transparently because the row carries `provider`. Routes here cover
//! ingesting an `auth.json` blob and picking a credential for per-process
//! env injection (never writes `~/.codex/auth.json`).

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use settings::CodexCredentialError;

use crate::response::{error_response, internal_error, ApiCreated, ApiError};
use crate::{ApiRouter, AppState};

pub(crate) fn codex_credential_routes() -> ApiRouter<AppState> {
    let router = ApiRouter::new();
    let router = crate::api_route!(
        router,
        POST "/api/credentials/codex",
        transport = Json,
        auth = Protected,
        handler = add_codex_credential,
        body = api_types::AddCodexCredentialRequest,
        res = api_types::AddCodexCredentialResponse
    );
    let router = crate::api_route!(
        router,
        POST "/api/credentials/codex/pick",
        transport = Json,
        auth = Protected,
        handler = pick_codex_credential,
        body = api_types::EmptyRequest,
        res = api_types::CodexCredentialPickResponse
    );
    crate::api_route!(
        router,
        POST "/api/credentials/codex/sync",
        transport = Json,
        auth = Protected,
        handler = sync_codex_credential,
        body = api_types::SyncCodexCredentialRequest,
        res = api_types::SyncCodexCredentialResponse
    )
}

/// POST /api/credentials/codex — paste an auth.json blob, validate, probe,
/// store. Returns 201 with the new credential id, account_id, plan_type.
#[crate::instrument_api(method = "POST", path = "/api/credentials/codex")]
async fn add_codex_credential(
    State(state): State<AppState>,
    Json(body): Json<api_types::AddCodexCredentialRequest>,
) -> Result<ApiCreated<api_types::AddCodexCredentialResponse>, ApiError> {
    let label = body.label.trim().to_string();
    if label.is_empty() {
        return Err(error_response(StatusCode::BAD_REQUEST, "label is required"));
    }
    let auth_json_text = body.auth_json.trim();
    if auth_json_text.is_empty() {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "auth_json is required",
        ));
    }
    match state
        .settings
        .store_codex_credential(&label, auth_json_text)
        .await
    {
        Ok(stored) => {
            state.bus.send(global_bus::BusPayload::Credentials(None));
            Ok(ApiCreated(api_types::AddCodexCredentialResponse {
                ok: true,
                id: stored.id,
                label,
                account_id: stored.account_id,
                plan_type: stored.plan_type,
            }))
        }
        Err(CodexCredentialError::AuthJson(e)) => Err(error_response(
            StatusCode::BAD_REQUEST,
            &format!("invalid auth.json: {e}"),
        )),
        Err(CodexCredentialError::NoAccountId) => Err(error_response(
            StatusCode::BAD_REQUEST,
            "auth.json has no account_id and the JWT carries no chatgpt_account_id claim",
        )),
        Err(CodexCredentialError::DuplicateLabel(label, _)) => Err(error_response(
            StatusCode::CONFLICT,
            &format!("credential label {label:?} already exists"),
        )),
        Err(CodexCredentialError::DuplicateAccount(account, id)) => Err(error_response(
            StatusCode::CONFLICT,
            &format!("a Codex credential for account {account} already exists (id={id})"),
        )),
        Err(CodexCredentialError::PermanentRefreshFailure(reason)) => Err(error_response(
            StatusCode::UNAUTHORIZED,
            &format!("stored refresh token permanently invalid ({reason}); re-add the credential"),
        )),
        Err(CodexCredentialError::Probe(e)) => Err(error_response(
            StatusCode::BAD_GATEWAY,
            &format!("upstream usage probe failed: {e}"),
        )),
        Err(e) => Err(internal_error(
            anyhow::Error::msg(e.to_string()),
            "failed to store codex credential",
        )),
    }
}

/// POST /api/credentials/codex/pick — return the best-available Codex
/// credential for per-process env injection. Refreshes stale tokens before
/// returning; never writes `~/.codex/auth.json`.
#[crate::instrument_api(method = "POST", path = "/api/credentials/codex/pick")]
async fn pick_codex_credential(
    State(state): State<AppState>,
    Json(_body): Json<api_types::EmptyRequest>,
) -> Result<Json<api_types::CodexCredentialPickResponse>, ApiError> {
    match state.settings.pick_codex_credential().await {
        Ok(pick) => Ok(Json(api_types::CodexCredentialPickResponse {
            pick: pick.map(|p| api_types::CodexCredentialPick {
                id: p.id,
                label: p.label,
                access_token: p.access_token,
                account_id: p.account_id,
                auth_json: p.auth_json,
            }),
        })),
        Err(CodexCredentialError::PermanentRefreshFailure(reason)) => Err(error_response(
            StatusCode::UNAUTHORIZED,
            &format!("stored refresh token permanently invalid ({reason}); re-add the credential",),
        )),
        Err(e) => Err(internal_error(
            anyhow::Error::msg(e.to_string()),
            "failed to pick codex credential",
        )),
    }
}

/// POST /api/credentials/codex/sync — read refreshed tokens from a temp
/// `CODEX_HOME/auth.json` and persist them on the picked credential row.
#[crate::instrument_api(method = "POST", path = "/api/credentials/codex/sync")]
async fn sync_codex_credential(
    State(state): State<AppState>,
    Json(body): Json<api_types::SyncCodexCredentialRequest>,
) -> Result<Json<api_types::SyncCodexCredentialResponse>, ApiError> {
    let auth_json_text = body.auth_json.trim();
    if auth_json_text.is_empty() {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "auth_json is required",
        ));
    }
    match state
        .settings
        .sync_codex_credential(body.credential_id, auth_json_text)
        .await
    {
        Ok(()) => {
            state.bus.send(global_bus::BusPayload::Credentials(None));
            Ok(Json(api_types::SyncCodexCredentialResponse { ok: true }))
        }
        Err(CodexCredentialError::AuthJson(e)) => Err(error_response(
            StatusCode::BAD_REQUEST,
            &format!("invalid auth.json: {e}"),
        )),
        Err(CodexCredentialError::NoAccountId) => Err(error_response(
            StatusCode::BAD_REQUEST,
            "auth.json has no account_id and the JWT carries no chatgpt_account_id claim",
        )),
        Err(CodexCredentialError::NotCodex) => Err(error_response(
            StatusCode::BAD_REQUEST,
            "credential is not a Codex row",
        )),
        Err(CodexCredentialError::AccountMismatch { expected, got }) => Err(error_response(
            StatusCode::BAD_REQUEST,
            &format!("auth.json account_id {got} does not match stored credential ({expected})"),
        )),
        Err(CodexCredentialError::NotFound(id)) => Err(error_response(
            StatusCode::NOT_FOUND,
            &format!("credential id={id} not found"),
        )),
        Err(e) => Err(internal_error(
            anyhow::Error::msg(e.to_string()),
            "failed to sync codex credential",
        )),
    }
}
