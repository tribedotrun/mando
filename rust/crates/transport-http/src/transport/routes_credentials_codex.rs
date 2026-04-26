//! Codex-specific credential routes — POST add, GET active, POST activate.
//!
//! See PR #1006. The list / probe / delete / token paths in
//! `routes_credentials.rs` already handle Codex rows transparently because
//! the row carries `provider`. The routes here are purely Codex-specific
//! actions: ingesting an `auth.json` blob, telling the UI which row is
//! currently active, and writing a row's tokens to `~/.codex/auth.json`.

use axum::extract::{Path, State};
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
        GET "/api/credentials/codex/active",
        transport = Json,
        auth = Protected,
        handler = get_codex_active,
        res = api_types::CodexActiveResponse
    );
    crate::api_route!(
        router,
        POST "/api/credentials/{id}/codex-activate",
        transport = Json,
        auth = Protected,
        handler = activate_codex_credential,
        body = api_types::EmptyRequest,
        params = api_types::CredentialIdParams,
        res = api_types::CodexActivateResponse
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
        Err(CodexCredentialError::DuplicateAccount(account_id, existing_id)) => {
            Err(error_response(
                StatusCode::CONFLICT,
                &format!(
                    "a Codex credential for account {account_id} already exists (id={existing_id})"
                ),
            ))
        }
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

/// GET /api/credentials/codex/active — read `~/.codex/auth.json` and report
/// the active account_id plus the matching stored credential id (if any).
#[crate::instrument_api(method = "GET", path = "/api/credentials/codex/active")]
async fn get_codex_active(
    State(state): State<AppState>,
) -> Result<Json<api_types::CodexActiveResponse>, ApiError> {
    match state.settings.get_codex_active_account().await {
        Ok((active_account_id, matched_credential_id)) => {
            Ok(Json(api_types::CodexActiveResponse {
                active_account_id,
                matched_credential_id,
            }))
        }
        Err(e) => Err(internal_error(e, "failed to read codex active account")),
    }
}

/// POST /api/credentials/{id}/codex-activate — write the row's tokens to
/// `~/.codex/auth.json`. Refresh-on-stale handled inside settings.
#[crate::instrument_api(method = "POST", path = "/api/credentials/{id}/codex-activate")]
async fn activate_codex_credential(
    State(state): State<AppState>,
    Path(api_types::CredentialIdParams { id }): Path<api_types::CredentialIdParams>,
    Json(_body): Json<api_types::EmptyRequest>,
) -> Result<Json<api_types::CodexActivateResponse>, ApiError> {
    match state.settings.activate_codex_credential(id).await {
        Ok(account_id) => {
            state.bus.send(global_bus::BusPayload::Credentials(None));
            Ok(Json(api_types::CodexActivateResponse {
                ok: true,
                account_id,
            }))
        }
        Err(CodexCredentialError::NotFound(_)) => Err(error_response(
            StatusCode::NOT_FOUND,
            "credential not found",
        )),
        Err(CodexCredentialError::NotCodex) => Err(error_response(
            StatusCode::BAD_REQUEST,
            "credential is not a Codex credential",
        )),
        Err(CodexCredentialError::MissingTokens) => Err(error_response(
            StatusCode::CONFLICT,
            "credential is missing the required token fields; re-add it",
        )),
        Err(CodexCredentialError::PermanentRefreshFailure(reason)) => Err(error_response(
            StatusCode::UNAUTHORIZED,
            &format!("stored refresh token permanently invalid ({reason}); re-add the credential",),
        )),
        Err(e) => Err(internal_error(
            anyhow::Error::msg(e.to_string()),
            "failed to activate codex credential",
        )),
    }
}
