//! Route for updating a Claude credential's access token in place. Split
//! out from `routes_credentials.rs` to keep that file under the
//! file-length budget.
//!
//! POST shares the `/api/credentials/{id}/token` path with the existing GET
//! (token reveal) — axum merges method routers registered on the same path,
//! including across `Router::merge`, so the two declarations coexist.

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;

use crate::credentials_oauth::decode_jwt_expiry;
use crate::response::{error_response, internal_error, ApiError};
use crate::{ApiRouter, AppState};

pub(crate) fn credential_update_routes() -> ApiRouter<AppState> {
    crate::api_route!(
        ApiRouter::new(),
        POST "/api/credentials/{id}/token",
        transport = Json,
        auth = Protected,
        handler = update_credential_token,
        body = api_types::UpdateCredentialTokenRequest,
        params = api_types::CredentialIdParams,
        res = api_types::UpdateCredentialTokenResponse
    )
}

/// POST /api/credentials/{id}/token -- replace the stored access token on
/// an existing Claude credential in place (same label, same row).
/// Overwrites `expires_at` with the new token's decoded expiry — which
/// un-expires rows previously stamped expired after a 401 probe — and
/// clears any rate-limit cooldown.
async fn update_credential_token(
    State(state): State<AppState>,
    axum::extract::Path(api_types::CredentialIdParams { id }): axum::extract::Path<
        api_types::CredentialIdParams,
    >,
    Json(body): Json<api_types::UpdateCredentialTokenRequest>,
) -> Result<Json<api_types::UpdateCredentialTokenResponse>, ApiError> {
    let token = body.token.trim().to_string();
    if token.is_empty() {
        return Err(error_response(StatusCode::BAD_REQUEST, "token is required"));
    }
    let expires_at = decode_jwt_expiry(&token);

    match state
        .settings
        .update_credential_token(id, &token, expires_at)
        .await
    {
        Ok(Some(label)) => {
            state.bus.send(global_bus::BusPayload::Credentials(None));
            Ok(Json(api_types::UpdateCredentialTokenResponse {
                ok: true,
                id,
                label,
            }))
        }
        Ok(None) => Err(error_response(
            StatusCode::NOT_FOUND,
            &format!("no Claude credential with id={id}"),
        )),
        Err(e) => Err(internal_error(e, "failed to update credential token")),
    }
}
