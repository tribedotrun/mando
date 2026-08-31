//! API routes for credential management.

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use settings::{ProbeError, RateLimitStatus as ProbeRateLimitStatus};

use crate::credentials_oauth::decode_jwt_expiry;
use crate::response::{error_response, internal_error, internal_error_with, ApiCreated, ApiError};
use crate::{ApiRouter, AppState};

pub(crate) fn credential_routes() -> ApiRouter<AppState> {
    let router = ApiRouter::new();
    let router = crate::api_route!(
        router,
        GET "/api/credentials",
        transport = Json,
        auth = Protected,
        handler = list_credentials,
        res = api_types::CredentialsListResponse
    );
    let router = crate::api_route!(
        router,
        DELETE "/api/credentials/{id}",
        transport = Json,
        auth = Protected,
        handler = remove_credential,
        params = api_types::CredentialIdParams,
        res = api_types::CredentialMutationResponse
    );
    let router = crate::api_route!(
        router,
        POST "/api/credentials/{id}/disable",
        transport = Json,
        auth = Protected,
        handler = disable_credential,
        body = api_types::EmptyRequest,
        params = api_types::CredentialIdParams,
        res = api_types::CredentialMutationResponse
    );
    let router = crate::api_route!(
        router,
        POST "/api/credentials/{id}/enable",
        transport = Json,
        auth = Protected,
        handler = enable_credential,
        body = api_types::EmptyRequest,
        params = api_types::CredentialIdParams,
        res = api_types::CredentialMutationResponse
    );
    let router = crate::api_route!(
        router,
        GET "/api/credentials/{id}/token",
        transport = Json,
        auth = Protected,
        handler = get_credential_token,
        params = api_types::CredentialIdParams,
        res = api_types::CredentialTokenResponse
    );
    let router = crate::api_route!(
        router,
        POST "/api/credentials/{id}/probe",
        transport = Json,
        auth = Protected,
        handler = probe_credential,
        body = api_types::EmptyRequest,
        params = api_types::CredentialIdParams,
        res = api_types::ProbeCredentialResponse
    );
    let router = crate::api_route!(
        router,
        POST "/api/credentials/setup-token",
        transport = Json,
        auth = Protected,
        handler = add_setup_token,
        body = api_types::SetupTokenRequest,
        res = api_types::SetupTokenResponse
    );
    crate::api_route!(
        router,
        POST "/api/credentials/pick",
        transport = Json,
        auth = Protected,
        handler = pick_credential,
        body = api_types::CredentialPickRequest,
        res = api_types::CredentialPickResponse
    )
}

fn provider_to_api(provider: &str) -> api_types::CredentialProvider {
    match provider {
        "codex" => api_types::CredentialProvider::Codex,
        _ => api_types::CredentialProvider::Claude,
    }
}

fn api_rate_limit_status(status: &str) -> api_types::CredentialRateLimitStatus {
    match status {
        "allowed_warning" => api_types::CredentialRateLimitStatus::AllowedWarning,
        "rejected" => api_types::CredentialRateLimitStatus::Rejected,
        _ => api_types::CredentialRateLimitStatus::Allowed,
    }
}

fn api_probe_rate_limit_status(
    status: ProbeRateLimitStatus,
) -> api_types::CredentialRateLimitStatus {
    match status {
        ProbeRateLimitStatus::Allowed => api_types::CredentialRateLimitStatus::Allowed,
        ProbeRateLimitStatus::AllowedWarning => {
            api_types::CredentialRateLimitStatus::AllowedWarning
        }
        ProbeRateLimitStatus::Rejected => api_types::CredentialRateLimitStatus::Rejected,
        // Unknown upstream tags map to `Allowed` at the wire contract — the
        // UI has no projection for unknowns and we don't want to silently
        // report a rate-limited state based on an unrecognized value.
        ProbeRateLimitStatus::Unknown(_) => api_types::CredentialRateLimitStatus::Allowed,
    }
}

/// GET /api/credentials -- list all stored credentials (no secrets).
async fn list_credentials(
    State(state): State<AppState>,
) -> Json<api_types::CredentialsListResponse> {
    let creds = state
        .settings
        .list_credentials()
        .await
        .into_iter()
        .map(|cred| api_types::CredentialInfo {
            id: cred.id,
            label: cred.label,
            token_masked: cred.token_masked,
            provider: provider_to_api(&cred.provider),
            expires_at: cred.expires_at,
            rate_limit_cooldown_until: cred.rate_limit_cooldown_until,
            disabled_at: cred.disabled_at,
            created_at: cred.created_at,
            is_expired: cred.is_expired,
            is_rate_limited: cred.is_rate_limited,
            is_disabled: cred.is_disabled,
            five_hour: cred
                .five_hour
                .map(|window| api_types::CredentialWindowInfo {
                    utilization: window.utilization,
                    reset_at: window.reset_at,
                    status: api_rate_limit_status(&window.status),
                }),
            seven_day: cred
                .seven_day
                .map(|window| api_types::CredentialWindowInfo {
                    utilization: window.utilization,
                    reset_at: window.reset_at,
                    status: api_rate_limit_status(&window.status),
                }),
            unified_status: cred.unified_status.as_deref().map(api_rate_limit_status),
            representative_claim: cred.representative_claim,
            last_probed_at: cred.last_probed_at,
            cost_since_probe_usd: cred.cost_since_probe_usd,
            codex: cred.codex.map(|c| api_types::CodexCredentialDetails {
                account_id: c.account_id,
                plan_type: c.plan_type,
                credits_balance: c.credits_balance,
                credits_unlimited: c.credits_unlimited,
            }),
        })
        .collect();
    Json(api_types::CredentialsListResponse { credentials: creds })
}

/// GET /api/credentials/:id/token -- reveal the full token.
async fn get_credential_token(
    State(state): State<AppState>,
    axum::extract::Path(api_types::CredentialIdParams { id }): axum::extract::Path<
        api_types::CredentialIdParams,
    >,
) -> Result<Json<api_types::CredentialTokenResponse>, ApiError> {
    match state.settings.get_credential_token(id).await {
        Ok(Some(token)) => Ok(Json(api_types::CredentialTokenResponse { token })),
        Ok(None) => Err(error_response(StatusCode::NOT_FOUND, "not found")),
        Err(e) => Err(internal_error(e, "failed to read credential token")),
    }
}

/// DELETE /api/credentials/:id -- remove a credential.
async fn remove_credential(
    State(state): State<AppState>,
    axum::extract::Path(api_types::CredentialIdParams { id }): axum::extract::Path<
        api_types::CredentialIdParams,
    >,
) -> Result<Json<api_types::CredentialMutationResponse>, ApiError> {
    match state.settings.remove_credential(id).await {
        Ok(true) => {
            state.bus.send(global_bus::BusPayload::Credentials(None));
            Ok(Json(api_types::CredentialMutationResponse {
                ok: true,
                error: None,
            }))
        }
        Ok(false) => Err(error_response(StatusCode::NOT_FOUND, "not found")),
        Err(e) => Err(internal_error(e, "failed to remove credential")),
    }
}

async fn disable_credential(
    State(state): State<AppState>,
    axum::extract::Path(api_types::CredentialIdParams { id }): axum::extract::Path<
        api_types::CredentialIdParams,
    >,
    Json(_body): Json<api_types::EmptyRequest>,
) -> Result<Json<api_types::CredentialMutationResponse>, ApiError> {
    set_credential_disabled(state, id, true).await
}

async fn enable_credential(
    State(state): State<AppState>,
    axum::extract::Path(api_types::CredentialIdParams { id }): axum::extract::Path<
        api_types::CredentialIdParams,
    >,
    Json(_body): Json<api_types::EmptyRequest>,
) -> Result<Json<api_types::CredentialMutationResponse>, ApiError> {
    set_credential_disabled(state, id, false).await
}

async fn set_credential_disabled(
    state: AppState,
    id: i64,
    disabled: bool,
) -> Result<Json<api_types::CredentialMutationResponse>, ApiError> {
    match state.settings.set_credential_disabled(id, disabled).await {
        Ok(true) => {
            state.bus.send(global_bus::BusPayload::Credentials(None));
            Ok(Json(api_types::CredentialMutationResponse {
                ok: true,
                error: None,
            }))
        }
        Ok(false) => Err(error_response(StatusCode::NOT_FOUND, "not found")),
        Err(e) => Err(internal_error(
            e,
            "failed to update credential disabled state",
        )),
    }
}

/// POST /api/credentials/:id/probe -- force an immediate usage probe.
///
/// Returns the fresh snapshot. On 401 the credential is marked expired.
/// Emits `BusPayload::Credentials` on every outcome (success, 401, and
/// transient probe failure) so the UI always refetches — transient
/// failures still advance `last_probed_at` elsewhere and users benefit
/// from seeing the current snapshot even when the probe itself errors.
async fn probe_credential(
    State(state): State<AppState>,
    axum::extract::Path(api_types::CredentialIdParams { id }): axum::extract::Path<
        api_types::CredentialIdParams,
    >,
    Json(_body): Json<api_types::EmptyRequest>,
) -> Result<Json<api_types::ProbeCredentialResponse>, ApiError> {
    let row = match state.settings.get_credential_row(id).await {
        Ok(Some(row)) => row,
        Ok(None) => {
            return Err(error_response(
                StatusCode::NOT_FOUND,
                "credential not found",
            ));
        }
        Err(e) => {
            return Err(internal_error(e, "failed to read credential"));
        }
    };
    match state.captain.probe_credential_usage(&row).await {
        Ok(snapshot) => {
            state.bus.send(global_bus::BusPayload::Credentials(None));
            Ok(Json(api_types::ProbeCredentialResponse {
                ok: true,
                snapshot: Some(api_types::CredentialUsageSnapshot {
                    five_hour: api_types::UsageWindowState {
                        utilization: snapshot.five_hour.utilization,
                        reset_at: snapshot.five_hour.reset_at,
                        status: api_probe_rate_limit_status(snapshot.five_hour.status),
                    },
                    seven_day: api_types::UsageWindowState {
                        utilization: snapshot.seven_day.utilization,
                        reset_at: snapshot.seven_day.reset_at,
                        status: api_probe_rate_limit_status(snapshot.seven_day.status),
                    },
                    unified_status: api_probe_rate_limit_status(snapshot.unified_status),
                    representative_claim: snapshot.representative_claim,
                    probed_at: snapshot.probed_at,
                }),
                error: None,
            }))
        }
        Err(ProbeError::Unauthorized) => {
            if let Err(e) = state.settings.mark_credential_expired(id).await {
                tracing::warn!(
                    module = "credentials",
                    credential_id = id,
                    error = %e,
                    "failed to mark credential expired after 401 probe"
                );
            }
            state.bus.send(global_bus::BusPayload::Credentials(None));
            Err(error_response(
                StatusCode::UNAUTHORIZED,
                "token expired or invalid; re-login required",
            ))
        }
        Err(e) => {
            state.bus.send(global_bus::BusPayload::Credentials(None));
            Err(internal_error_with(
                StatusCode::BAD_GATEWAY,
                e,
                "upstream credential probe failed",
            ))
        }
    }
}

/// POST /api/credentials/pick -- return the best-available credential for an
/// external caller (e.g. a `claude` invocation in a user terminal).
///
/// Reuses the same `pick_for_worker` selection that captain uses: ordered by
/// fewest active sessions, then lowest five-hour utilization, then id;
/// expired and cooling-down credentials are excluded. Ranking counts every
/// running session on a credential, so terminal use balances against worker
/// load out of the one global pool.
///
/// Returns `{ pick: null }` (HTTP 200) when nothing is usable — the caller is
/// expected to fall back to its ambient login rather than treat that as an
/// error.
async fn pick_credential(
    State(state): State<AppState>,
    Json(body): Json<api_types::CredentialPickRequest>,
) -> Result<Json<api_types::CredentialPickResponse>, ApiError> {
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
    let pick = if body.id.is_some() || explicit_label.is_some() {
        state
            .settings
            .pick_claude_credential_explicit(body.id, explicit_label)
            .await
            .map_err(|e| internal_error(e, "failed to pick credential"))?
            .map(|(id, token, label)| api_types::CredentialPick { id, label, token })
    } else {
        match state
            .settings
            .pick_worker_credential()
            .await
            .map_err(|e| internal_error(e, "failed to pick credential"))?
        {
            None => None,
            Some((id, token)) => {
                let labels = state
                    .settings
                    .credential_labels_by_ids(&[id])
                    .await
                    .map_err(|e| internal_error(e, "failed to load credential label"))?;
                let label = labels.get(&id).cloned().unwrap_or_else(|| id.to_string());
                Some(api_types::CredentialPick { id, label, token })
            }
        }
    };

    Ok(Json(api_types::CredentialPickResponse { pick }))
}

/// POST /api/credentials/setup-token -- add a setup-token credential.
async fn add_setup_token(
    State(state): State<AppState>,
    Json(body): Json<api_types::SetupTokenRequest>,
) -> Result<ApiCreated<api_types::SetupTokenResponse>, ApiError> {
    let label = body.label.trim().to_string();
    let token = body.token.trim().to_string();
    if label.is_empty() || token.is_empty() {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "label and token are required",
        ));
    }

    let expires_at = decode_jwt_expiry(&token);

    match state.settings.find_credential_by_label(&label).await {
        Ok(Some(existing_id)) => {
            return Err(error_response(
                StatusCode::CONFLICT,
                &format!(
                    "label {label:?} is already in use by an existing credential (id={existing_id})"
                ),
            ));
        }
        Ok(None) => {}
        Err(e) => return Err(internal_error(e, "failed to check credential label")),
    }

    match state
        .settings
        .store_credential(&label, &token, expires_at)
        .await
    {
        Ok(id) => {
            state.bus.send(global_bus::BusPayload::Credentials(None));
            Ok(ApiCreated(api_types::SetupTokenResponse {
                ok: true,
                id: Some(id),
                label: Some(label),
            }))
        }
        Err(e) => {
            // Race window: a concurrent add with the same label could slip
            // past the pre-check above and lose to the table-wide UNIQUE
            // constraint here. Re-query so the racing caller still sees 409
            // instead of a generic 500.
            if let Ok(Some(existing_id)) = state.settings.find_credential_by_label(&label).await {
                return Err(error_response(
                    StatusCode::CONFLICT,
                    &format!(
                        "label {label:?} is already in use by an existing credential (id={existing_id})"
                    ),
                ));
            }
            Err(internal_error(e, "failed to store credential"))
        }
    }
}
