//! Provider-aware credential probe dispatcher.
//!
//! See PR #1006. Routes by `CredentialRow.provider`:
//!
//! - `claude` → `usage_probe::probe(access_token)` and the existing header-
//!   parse path. No token refresh; Claude OAuth tokens don't have a
//!   refresh-token equivalent.
//! - `codex` → optional proactive `codex_oauth_refresh::refresh()` (if the
//!   stored JWT exp is within 5 min or `last_probed_at` is older than 7
//!   days), then `codex_probe::probe()`. On 401 from the probe itself, one
//!   reactive refresh + retry as a safety net.
//!
//! Returns the same `UsageSnapshot` the Claude path produces so
//! `pick_for_worker`'s SQL keeps one query.

use sqlx::SqlitePool;
use tracing::{debug, warn};

use crate::io::codex_credentials;
use crate::io::codex_oauth_refresh;
use crate::io::codex_probe;
use crate::io::credentials::CredentialRow;
use crate::io::usage_probe::{self, ProbeError, UsageSnapshot};

/// Probe a credential and return its current usage snapshot. Side effects:
///
/// - Refreshed Codex tokens are persisted via `update_codex_tokens`.
/// - `plan_type` and `credits_*` are persisted via `update_codex_plan_and_credits`.
/// - `last_probed_at` and the snapshot columns are written by the caller
///   (this function returns the snapshot but does not call
///   `set_usage_snapshot` itself, mirroring the existing Claude path).
pub async fn probe(pool: &SqlitePool, row: &CredentialRow) -> Result<UsageSnapshot, ProbeError> {
    match row.provider.as_str() {
        "codex" => probe_codex(pool, row).await,
        _ => usage_probe::probe(&row.access_token).await,
    }
}

async fn probe_codex(pool: &SqlitePool, row: &CredentialRow) -> Result<UsageSnapshot, ProbeError> {
    let now_secs = time::OffsetDateTime::now_utc().unix_timestamp();
    let id_token_exp_secs =
        row.id_token
            .as_deref()
            .and_then(|t| match codex_credentials::decode_id_token_claims(t) {
                Ok(claims) => claims.exp,
                Err(err) => {
                    debug!(
                        module = "settings",
                        credential_id = row.id,
                        error = %err,
                        "id_token decode failed; proactive refresh exp check skipped"
                    );
                    None
                }
            });

    let mut access_token = row.access_token.clone();
    let mut did_proactive_refresh = false;

    if codex_oauth_refresh::should_refresh(
        id_token_exp_secs,
        row.token_updated_at.or(row.last_probed_at),
        now_secs,
    ) {
        match try_refresh_and_persist(pool, row, false).await {
            Ok(Some(new_access)) => {
                access_token = new_access;
                did_proactive_refresh = true;
            }
            Ok(None) => {
                // No refresh_token stored — proceed with the existing
                // access_token; the probe will return Unauthorized if it
                // is dead and the caller marks the row expired.
            }
            Err(err) => {
                if matches!(err, ProbeError::Unauthorized) {
                    return Err(err);
                }
                warn!(
                    module = "settings",
                    credential_id = row.id,
                    error = %err,
                    "proactive Codex token refresh failed; probing with stale access_token"
                );
            }
        }
    }

    match probe_with(pool, row, &access_token).await {
        Err(ProbeError::Unauthorized) if did_proactive_refresh => {
            // We just refreshed and the freshly-issued token still 401s.
            // Do NOT call refresh again — OpenAI invalidates the new
            // refresh_token on reuse, which would mark a live credential
            // permanently dead. Surface this as transient so callers do not
            // mark the refreshed credential expired.
            warn!(
                module = "settings",
                credential_id = row.id,
                "Codex probe 401 immediately after proactive refresh; \
                 treating as retry-later to avoid refresh_token_reused"
            );
            Err(ProbeError::Network(
                "Codex probe 401 immediately after proactive refresh".to_string(),
            ))
        }
        Err(ProbeError::Unauthorized) => match try_refresh_and_persist(pool, row, true).await? {
            Some(new_access) => probe_with(pool, row, &new_access).await,
            None => Err(ProbeError::Unauthorized),
        },
        result => result,
    }
}

/// Run the Codex probe with the given access_token and persist the
/// plan/credits side effects on success. Returns the snapshot.
async fn probe_with(
    pool: &SqlitePool,
    row: &CredentialRow,
    access_token: &str,
) -> Result<UsageSnapshot, ProbeError> {
    let outcome = codex_probe::probe(access_token, row.account_id.as_deref()).await?;
    if let Err(err) = codex_credentials::update_codex_plan_and_credits(
        pool,
        row.id,
        outcome.plan_type.as_deref(),
        outcome.credits_balance.as_deref(),
        outcome.credits_unlimited,
    )
    .await
    {
        warn!(
            module = "settings",
            credential_id = row.id,
            error = %err,
            "failed to persist Codex plan/credits info"
        );
    }
    Ok(outcome.snapshot)
}

/// Attempt a proactive or reactive refresh and persist the result. Returns
/// the new access_token on success, `None` when the row has no
/// refresh_token to use, or a `ProbeError` for permanent / transient
/// failures (mapped from `RefreshError`).
async fn try_refresh_and_persist(
    pool: &SqlitePool,
    row: &CredentialRow,
    force: bool,
) -> Result<Option<String>, ProbeError> {
    codex_oauth_refresh::with_credential_refresh_lock(row.id, || async {
        let current = crate::io::credentials::get_row_by_id(pool, row.id)
            .await
            .map_err(|e| ProbeError::Persist(e.to_string()))?
            .ok_or_else(|| ProbeError::Persist(format!("credential id={} not found", row.id)))?;
        if current.access_token != row.access_token {
            return Ok(Some(current.access_token));
        }
        if !force {
            let now_secs = time::OffsetDateTime::now_utc().unix_timestamp();
            let exp_secs = current.id_token.as_deref().and_then(|token| {
                codex_credentials::decode_id_token_claims(token)
                    .ok()
                    .and_then(|claims| claims.exp)
            });
            if !codex_oauth_refresh::should_refresh(
                exp_secs,
                current.token_updated_at.or(current.last_probed_at),
                now_secs,
            ) {
                return Ok(Some(current.access_token));
            }
        }
        let Some(refresh_token) = current.refresh_token.as_deref() else {
            return Ok(None);
        };
        match codex_oauth_refresh::refresh(refresh_token).await {
            Ok(refreshed) => {
                // expires_at stays None on refresh — JWT exp tracks the
                // short-lived id_token, not the credential's "is dead?"
                // signal. is_expired is only flipped via mark_expired on
                // permanent refresh failure.
                if let Err(err) = codex_credentials::update_codex_tokens(
                    pool,
                    current.id,
                    &refreshed.access_token,
                    &refreshed.refresh_token,
                    refreshed.id_token.as_deref(),
                    None,
                )
                .await
                {
                    return Err(ProbeError::Persist(err.to_string()));
                }
                Ok(Some(refreshed.access_token))
            }
            Err(codex_oauth_refresh::RefreshError::Permanent(reason)) => {
                warn!(
                    module = "settings",
                    credential_id = current.id,
                    reason,
                    "Codex refresh token permanently invalid; marking expired"
                );
                Err(ProbeError::Unauthorized)
            }
            Err(codex_oauth_refresh::RefreshError::Unauthorized) => Err(ProbeError::Network(
                "Codex OAuth refresh returned 401 (transient; not marking expired)".to_string(),
            )),
            Err(codex_oauth_refresh::RefreshError::Network(msg)) => Err(ProbeError::Network(msg)),
            Err(codex_oauth_refresh::RefreshError::Http { status, body }) => {
                // Map upstream OAuth HTTP error to ProbeError::Http so the
                // poll loop logs it as a transport failure rather than
                // misdiagnosing a "response shape changed" parse error.
                warn!(
                    module = "settings",
                    credential_id = current.id,
                    status,
                    body = %body,
                    "Codex OAuth refresh returned non-success HTTP"
                );
                Err(ProbeError::Http(status))
            }
            Err(err @ codex_oauth_refresh::RefreshError::Parse(_)) => {
                Err(ProbeError::Parse(err.to_string()))
            }
        }
    })
    .await
}
