//! Pick-time refresh decision (Fix 5). Split out of
//! `codex_credentials_runtime.rs` to keep that file under the file length
//! budget.

use tracing::{debug, warn};

use crate::io::codex_credentials;
use crate::io::codex_oauth_refresh;
use crate::io::credentials;

use super::codex_credentials_runtime::CodexCredentialError;

/// Outcome of a single pick-time refresh attempt. `Expired` specifically
/// means this call marked the credential expired (a genuine transition
/// worth notifying on — pick candidates are pre-filtered to non-expired
/// rows), as opposed to `SkipTransient`, which just means the pick loop
/// should move on to the next candidate this round without concluding the
/// credential is dead.
pub(super) enum PickRefreshOutcome {
    Ready(String),
    Expired,
    SkipTransient,
}

#[tracing::instrument(skip(pool, row, access_token))]
pub(super) async fn refresh_codex_access_token_on_pick(
    pool: &sqlx::SqlitePool,
    id: i64,
    row: &credentials::CredentialRow,
    access_token: String,
) -> Result<PickRefreshOutcome, CodexCredentialError> {
    if !codex_row_should_refresh(row) {
        return Ok(PickRefreshOutcome::Ready(access_token));
    }

    codex_oauth_refresh::with_credential_refresh_lock(id, || async {
        let current = credentials::get_row_by_id(pool, id)
            .await?
            .ok_or(CodexCredentialError::NotFound(id))?;
        if current.provider != "codex" {
            return Err(CodexCredentialError::NotCodex);
        }
        if !codex_row_should_refresh(&current) {
            return Ok(PickRefreshOutcome::Ready(current.access_token));
        }
        let Some(refresh_token) = current.refresh_token.as_deref() else {
            warn!(
                module = "settings",
                credential_id = id,
                "Codex access_token needs refresh but refresh_token is missing; skipping pick"
            );
            return Ok(PickRefreshOutcome::SkipTransient);
        };

        match codex_oauth_refresh::refresh(refresh_token).await {
            Ok(refreshed) => {
                codex_credentials::update_codex_tokens(
                    pool,
                    id,
                    &refreshed.access_token,
                    &refreshed.refresh_token,
                    refreshed.id_token.as_deref(),
                    None,
                )
                .await
                .map_err(|e| CodexCredentialError::TokenPersistFailed(e.to_string()))?;
                Ok(PickRefreshOutcome::Ready(refreshed.access_token))
            }
            Err(codex_oauth_refresh::RefreshError::Permanent(reason)) => {
                warn!(
                    module = "settings",
                    credential_id = id,
                    reason = %reason,
                    "Codex refresh token permanently invalid on pick; marking expired"
                );
                if let Err(err) = credentials::mark_expired(pool, id).await {
                    warn!(
                        module = "settings",
                        credential_id = id,
                        error = %err,
                        "failed to mark Codex credential expired after permanent refresh failure"
                    );
                }
                Ok(PickRefreshOutcome::Expired)
            }
            Err(err) => {
                warn!(
                    module = "settings",
                    credential_id = id,
                    error = %err,
                    "Codex refresh failed transiently on pick; skipping credential"
                );
                Ok(PickRefreshOutcome::SkipTransient)
            }
        }
    })
    .await
}

fn codex_row_should_refresh(row: &credentials::CredentialRow) -> bool {
    let now_secs = time::OffsetDateTime::now_utc().unix_timestamp();
    let exp_secs = match codex_credentials::decode_id_token_claims(&row.access_token) {
        Ok(claims) => claims.exp,
        Err(err) => {
            debug!(
                module = "settings",
                credential_id = row.id,
                error = %err,
                "access_token decode failed; falling back to token age refresh check"
            );
            None
        }
    };
    codex_oauth_refresh::should_refresh(
        exp_secs,
        row.token_updated_at.or(row.last_probed_at),
        now_secs,
    )
}
