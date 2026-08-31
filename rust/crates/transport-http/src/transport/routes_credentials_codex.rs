//! Codex-specific credential routes — add, pick, sync, and reset credits.
//!
//! List / probe / delete paths in `routes_credentials.rs` handle Codex rows
//! transparently because the row carries `provider`. Routes here cover
//! ingesting an `auth.json` blob and picking a credential for per-process
//! env injection (never writes `~/.codex/auth.json`).

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use settings::{CodexCredentialError, ProbeError};

use crate::response::{error_response, internal_error, internal_error_with, ApiCreated, ApiError};
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
        body = api_types::CredentialPickRequest,
        res = api_types::CodexCredentialPickResponse
    );
    let router = crate::api_route!(
        router,
        POST "/api/credentials/codex/sync",
        transport = Json,
        auth = Protected,
        handler = sync_codex_credential,
        body = api_types::SyncCodexCredentialRequest,
        res = api_types::SyncCodexCredentialResponse
    );
    let router = crate::api_route!(
        router,
        POST "/api/credentials/codex/{id}/auth",
        transport = Json,
        auth = Protected,
        handler = update_codex_credential_auth,
        body = api_types::UpdateCodexCredentialAuthRequest,
        params = api_types::CredentialIdParams,
        res = api_types::AddCodexCredentialResponse
    );
    crate::api_route!(
        router,
        GET "/api/credentials/codex/{id}/reset-credits",
        transport = Json,
        auth = Protected,
        handler = get_codex_reset_credits,
        params = api_types::CredentialIdParams,
        res = api_types::CodexResetCreditsResponse
    )
}

/// GET /api/credentials/codex/:id/reset-credits — return available Codex
/// rate-limit reset credits for one stored OAuth credential.
async fn get_codex_reset_credits(
    State(state): State<AppState>,
    axum::extract::Path(api_types::CredentialIdParams { id }): axum::extract::Path<
        api_types::CredentialIdParams,
    >,
) -> Result<Json<api_types::CodexResetCreditsResponse>, ApiError> {
    match state.settings.codex_reset_credits(id).await {
        Ok(outcome) => Ok(Json(api_types::CodexResetCreditsResponse {
            available_count: outcome.available_count,
            total_earned_count: outcome.total_earned_count,
            credits: outcome
                .credits
                .into_iter()
                .map(|credit| api_types::CodexResetCredit {
                    title: credit.title,
                    description: credit.description,
                    expires_at: credit.expires_at,
                    granted_at: credit.granted_at,
                })
                .collect(),
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
        Err(CodexCredentialError::ResetCredits(ProbeError::Unauthorized))
        | Err(CodexCredentialError::Probe(ProbeError::Unauthorized)) => Err(error_response(
            StatusCode::UNAUTHORIZED,
            "token expired or invalid; re-login required",
        )),
        Err(CodexCredentialError::ResetCredits(e)) => Err(internal_error_with(
            StatusCode::BAD_GATEWAY,
            e,
            "upstream reset credits probe failed",
        )),
        Err(CodexCredentialError::Probe(e)) => Err(internal_error_with(
            StatusCode::BAD_GATEWAY,
            e,
            "upstream usage probe failed while refreshing Codex credential",
        )),
        Err(e) => Err(internal_error(
            anyhow::Error::msg(e.to_string()),
            "failed to read Codex reset credits",
        )),
    }
}

/// POST /api/credentials/codex — paste an auth.json blob, validate, probe,
/// store. Returns 201 with the new credential id, account_id, plan_type.
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
                warning: stored.warning,
            }))
        }
        Err(e) => Err(map_codex_store_error(e)),
    }
}

/// Map [`CodexCredentialError`] variants from the shared validate-then-store
/// pipeline to HTTP errors. Used by both the add endpoint and the row-scoped
/// auth update endpoint.
fn map_codex_store_error(e: CodexCredentialError) -> ApiError {
    match e {
        CodexCredentialError::AuthJson(e) => {
            error_response(StatusCode::BAD_REQUEST, &format!("invalid auth.json: {e}"))
        }
        CodexCredentialError::NoAccountId => error_response(
            StatusCode::BAD_REQUEST,
            "auth.json has no account_id and the JWT carries no chatgpt_account_id claim",
        ),
        CodexCredentialError::DuplicateLabel(label, _) => error_response(
            StatusCode::CONFLICT,
            &format!("credential label {label:?} already exists"),
        ),
        CodexCredentialError::DuplicateAccount(account, id) => error_response(
            StatusCode::CONFLICT,
            &format!("a Codex credential for account {account} already exists (id={id})"),
        ),
        CodexCredentialError::AmbientSessionConflict => error_response(
            StatusCode::BAD_REQUEST,
            "this is your live personal Codex login session (~/.codex); use Sign in with \
             browser in Settings -> Accounts, or capture manually: d=$(mktemp -d); \
             CODEX_HOME=\"$d\" codex login; then paste \"$d/auth.json\" and delete the dir \
             WITHOUT running codex logout",
        ),
        CodexCredentialError::PermanentRefreshFailure(reason) => error_response(
            StatusCode::BAD_REQUEST,
            &format!(
                "pasted session is invalid or revoked ({reason}); use Sign in with browser in \
                 Settings -> Accounts, or capture manually: d=$(mktemp -d); CODEX_HOME=\"$d\" \
                 codex login; then paste \"$d/auth.json\" and delete the dir WITHOUT running \
                 codex logout"
            ),
        ),
        CodexCredentialError::Probe(e) => error_response(
            StatusCode::BAD_GATEWAY,
            &format!("upstream usage probe failed: {e}"),
        ),
        e => internal_error(
            anyhow::Error::msg(e.to_string()),
            "failed to store codex credential",
        ),
    }
}

/// POST /api/credentials/codex/:id/auth — replace the stored session on an
/// existing Codex credential with a freshly captured auth.json for the SAME
/// ChatGPT account. Runs the full add pipeline against the row's existing
/// label (validate, force-refresh, upsert same row, probe usage).
async fn update_codex_credential_auth(
    State(state): State<AppState>,
    axum::extract::Path(api_types::CredentialIdParams { id }): axum::extract::Path<
        api_types::CredentialIdParams,
    >,
    Json(body): Json<api_types::UpdateCodexCredentialAuthRequest>,
) -> Result<Json<api_types::AddCodexCredentialResponse>, ApiError> {
    let auth_json_text = body.auth_json.trim();
    if auth_json_text.is_empty() {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "auth_json is required",
        ));
    }
    match state
        .settings
        .update_codex_credential_auth(id, auth_json_text)
        .await
    {
        Ok((label, stored)) => {
            state.bus.send(global_bus::BusPayload::Credentials(None));
            Ok(Json(api_types::AddCodexCredentialResponse {
                ok: true,
                id: stored.id,
                label,
                account_id: stored.account_id,
                plan_type: stored.plan_type,
                warning: stored.warning,
            }))
        }
        Err(CodexCredentialError::NotFound(id)) => Err(error_response(
            StatusCode::NOT_FOUND,
            &format!("credential id={id} not found"),
        )),
        Err(CodexCredentialError::NotCodex) => Err(error_response(
            StatusCode::BAD_REQUEST,
            "credential is not a Codex row",
        )),
        Err(CodexCredentialError::AccountMismatch { .. }) => Err(error_response(
            StatusCode::BAD_REQUEST,
            "pasted session belongs to a different ChatGPT account than this credential; add \
             it as a new account instead",
        )),
        Err(e) => Err(map_codex_store_error(e)),
    }
}

/// POST /api/credentials/codex/pick — return the best-available Codex
/// credential for per-process env injection. Refreshes stale or near-expired
/// tokens before returning; never writes `~/.codex/auth.json`. Fix 5: also
/// notifies (the same message the usage-poll poller uses) for every
/// candidate the pick walk itself marked expired.
async fn pick_codex_credential(
    State(state): State<AppState>,
    Json(body): Json<api_types::CredentialPickRequest>,
) -> Result<Json<api_types::CodexCredentialPickResponse>, ApiError> {
    if body.id.is_some() && body.label.is_some() {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "specify only one of id or label",
        ));
    }
    if let Some(label) = body.label.as_deref() {
        if label.trim().is_empty() {
            return Err(error_response(
                StatusCode::BAD_REQUEST,
                "label must not be empty",
            ));
        }
    }

    let explicit_label = body
        .label
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    if body.id.is_some() || explicit_label.is_some() {
        match state
            .settings
            .pick_codex_credential_explicit(body.id, explicit_label)
            .await
        {
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
                &format!(
                    "stored refresh token permanently invalid ({reason}); re-add the credential",
                ),
            )),
            Err(e) => Err(internal_error(
                anyhow::Error::msg(e.to_string()),
                "failed to pick codex credential",
            )),
        }
    } else {
        match state.settings.pick_codex_credential().await {
            Ok(outcome) => {
                for (id, label) in &outcome.newly_expired {
                    captain::notify_codex_credential_dead(&state.bus, *id, label);
                }
                Ok(Json(api_types::CodexCredentialPickResponse {
                    pick: outcome.pick.map(|p| api_types::CodexCredentialPick {
                        id: p.id,
                        label: p.label,
                        access_token: p.access_token,
                        account_id: p.account_id,
                        auth_json: p.auth_json,
                    }),
                }))
            }
            Err(CodexCredentialError::PermanentRefreshFailure(reason)) => Err(error_response(
                StatusCode::UNAUTHORIZED,
                &format!(
                    "stored refresh token permanently invalid ({reason}); re-add the credential",
                ),
            )),
            Err(e) => Err(internal_error(
                anyhow::Error::msg(e.to_string()),
                "failed to pick codex credential",
            )),
        }
    }
}

/// POST /api/credentials/codex/sync — read refreshed tokens from a temp
/// `CODEX_HOME/auth.json` and persist them on the picked credential row.
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
