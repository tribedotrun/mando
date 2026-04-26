//! Codex-specific credential-runtime methods (validate-then-store, read
//! the active account, write a credential's tokens to `~/.codex/auth.json`).
//!
//! Lives in its own module so `settings_runtime.rs` stays under the file
//! length limit.

use std::path::PathBuf;

use anyhow::Result;
use tracing::{debug, warn};

use crate::io::codex_credentials::{self, AuthJsonError, CodexJwtClaims};
use crate::io::codex_oauth_refresh;
use crate::io::codex_probe::{self, CodexProbeOutcome};
use crate::io::credentials;
use crate::io::usage_probe::ProbeError;

use super::settings_runtime::SettingsRuntime;

/// Outcome of a successful Codex credential add. Includes the parsed
/// `account_id` and `plan_type` for the API response so the UI can show
/// them without an extra round-trip.
#[derive(Debug, Clone)]
pub struct StoredCodexCredential {
    pub id: i64,
    pub account_id: String,
    pub plan_type: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum CodexCredentialError {
    #[error("auth.json invalid: {0}")]
    AuthJson(#[from] AuthJsonError),
    #[error("auth.json is missing tokens.account_id (no chatgpt_account_id JWT claim either)")]
    NoAccountId,
    #[error("a Codex credential for account {0} already exists (id={1})")]
    DuplicateAccount(String, i64),
    #[error("upstream usage probe failed: {0}")]
    Probe(#[from] ProbeError),
    #[error("database error: {0}")]
    Db(#[from] anyhow::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("auth.json serialize failed: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("refresh token permanently invalid (re-add the credential): {0}")]
    PermanentRefreshFailure(String),
    #[error("not a Codex credential")]
    NotCodex,
    #[error("codex credential is missing the required token fields")]
    MissingTokens,
    #[error("credential id={0} not found")]
    NotFound(i64),
}

impl SettingsRuntime {
    /// Validate-then-store a Codex `auth.json` blob. Parses the file,
    /// rejects non-chatgpt mode, decodes the JWT to extract `plan_type` +
    /// `account_id` (with fallback to the file's `tokens.account_id`),
    /// runs one synchronous usage probe to seed a snapshot, and inserts
    /// the row.
    #[tracing::instrument(skip(self, auth_json_text))]
    pub async fn store_codex_credential(
        &self,
        label: &str,
        auth_json_text: &str,
    ) -> Result<StoredCodexCredential, CodexCredentialError> {
        let parsed = codex_credentials::parse_auth_json(auth_json_text)?;
        let claims: CodexJwtClaims = codex_credentials::decode_id_token_claims(&parsed.id_token)?;
        let account_id = parsed
            .account_id
            .clone()
            .or_else(|| claims.account_id.clone())
            .ok_or(CodexCredentialError::NoAccountId)?;

        if let Some(existing_id) =
            codex_credentials::find_codex_id_by_account(&self.db_pool, &account_id).await?
        {
            return Err(CodexCredentialError::DuplicateAccount(
                account_id,
                existing_id,
            ));
        }

        // Probe immediately to confirm the tokens are live and seed
        // plan_type/credits state. If the probe says 401, surface that —
        // a freshly-pasted auth.json that's already dead is a paste mistake.
        let outcome: CodexProbeOutcome =
            codex_probe::probe(&parsed.access_token, Some(&account_id)).await?;
        let plan_type = outcome.plan_type.or(claims.plan_type);
        // expires_at is the credential-is-dead flag, not the access_token
        // expiry. The id_token JWT only lives a few hours; the refresh
        // token is the long-lived part. We only set expires_at when the
        // refresh path returns a permanent error (`mark_expired`), so
        // freshly-added Codex credentials always start un-expired.

        let id = codex_credentials::insert_codex(
            &self.db_pool,
            label,
            &parsed.access_token,
            &parsed.refresh_token,
            &parsed.id_token,
            &account_id,
            plan_type.as_deref(),
            None,
        )
        .await?;

        // Best-effort persistence of the snapshot fields. If it fails the
        // row is still in place; the next probe tick will fill them.
        global_infra::best_effort!(
            credentials::set_usage_snapshot(&self.db_pool, id, &outcome.snapshot).await,
            "codex_credentials_runtime: set_usage_snapshot on add"
        );
        global_infra::best_effort!(
            codex_credentials::update_codex_plan_and_credits(
                &self.db_pool,
                id,
                plan_type.as_deref(),
                outcome.credits_balance.as_deref(),
                outcome.credits_unlimited,
            )
            .await,
            "codex_credentials_runtime: update_codex_plan_and_credits on add"
        );

        Ok(StoredCodexCredential {
            id,
            account_id,
            plan_type,
        })
    }

    /// Read `~/.codex/auth.json`'s `account_id` and find a matching stored
    /// credential, if any. Used by the UI to render the "Active" badge.
    /// Surfaces parse / IO errors to the caller — the file genuinely being
    /// absent is the only `Ok((None, None))` case, so the badge can't
    /// silently lie when the file is corrupt or unreadable.
    #[tracing::instrument(skip(self))]
    pub async fn get_codex_active_account(&self) -> Result<(Option<String>, Option<i64>)> {
        let path = codex_credentials::default_auth_json_path();
        let active = codex_credentials::read_active_account_id(&path)
            .map_err(|e| anyhow::anyhow!("failed to read active codex account: {e}"))?;
        let matched = if let Some(ref acct) = active {
            codex_credentials::find_codex_id_by_account(&self.db_pool, acct).await?
        } else {
            None
        };
        Ok((active, matched))
    }

    /// Write a stored Codex credential's tokens to `~/.codex/auth.json`.
    /// Returns the activated `account_id`. Refreshes the access_token first
    /// when `should_refresh` says so, so the file we hand off contains
    /// fresh credentials.
    #[tracing::instrument(skip(self))]
    pub async fn activate_codex_credential(&self, id: i64) -> Result<String, CodexCredentialError> {
        let row = credentials::get_row_by_id(&self.db_pool, id)
            .await?
            .ok_or(CodexCredentialError::NotFound(id))?;
        if row.provider != "codex" {
            return Err(CodexCredentialError::NotCodex);
        }
        let (access_token, refresh_token, id_token, account_id) = match (
            row.refresh_token.clone(),
            row.id_token.clone(),
            row.account_id.clone(),
        ) {
            (Some(rt), Some(it), Some(acct)) => (row.access_token.clone(), rt, it, acct),
            _ => return Err(CodexCredentialError::MissingTokens),
        };

        // Proactive refresh: if the JWT exp is within 5 min or the row
        // hasn't been probed in 7 days, swap tokens before writing.
        let now_secs = time::OffsetDateTime::now_utc().unix_timestamp();
        let exp_secs = match codex_credentials::decode_id_token_claims(&id_token) {
            Ok(claims) => claims.exp,
            Err(err) => {
                debug!(
                    module = "settings",
                    credential_id = id,
                    error = %err,
                    "id_token decode failed; skipping proactive refresh exp check"
                );
                None
            }
        };
        let (final_access, final_refresh, final_id_token) =
            if codex_oauth_refresh::should_refresh(exp_secs, row.last_probed_at, now_secs) {
                match codex_oauth_refresh::refresh(&refresh_token).await {
                    Ok(refreshed) => {
                        let new_id = refreshed.id_token.clone().unwrap_or(id_token);
                        // Don't touch expires_at here either — see the
                        // store path. JWT exp != credential lifetime.
                        global_infra::best_effort!(
                            codex_credentials::update_codex_tokens(
                                &self.db_pool,
                                id,
                                &refreshed.access_token,
                                &refreshed.refresh_token,
                                refreshed.id_token.as_deref(),
                                None,
                            )
                            .await,
                            "codex_credentials_runtime: persist refreshed tokens on activate"
                        );
                        (refreshed.access_token, refreshed.refresh_token, new_id)
                    }
                    Err(codex_oauth_refresh::RefreshError::Permanent(reason)) => {
                        // Stored refresh_token is permanently dead. Bail
                        // before writing the now-useless tokens — re-adding
                        // the credential is the only fix.
                        warn!(
                            module = "settings",
                            credential_id = id,
                            reason = %reason,
                            "Codex refresh token permanently invalid on activate; refusing to write"
                        );
                        return Err(CodexCredentialError::PermanentRefreshFailure(reason));
                    }
                    Err(err) => {
                        // Transient refresh failure — fall back to the
                        // stored tokens. Codex's own 401 retry will
                        // surface a real failure on the user's next call,
                        // but we log it here so the operator sees it.
                        warn!(
                            module = "settings",
                            credential_id = id,
                            error = %err,
                            "Codex refresh failed transiently on activate; writing stored tokens"
                        );
                        (access_token, refresh_token, id_token)
                    }
                }
            } else {
                (access_token, refresh_token, id_token)
            };

        let path: PathBuf = codex_credentials::default_auth_json_path();
        // ISO8601 last_refresh so the Codex CLI's own refresh-interval
        // logic doesn't immediately re-refresh on next startup; it
        // checks `last_refresh` to decide.
        let last_refresh = time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .ok();
        let auth_json_text = codex_credentials::serialize_auth_json(
            &final_access,
            &final_refresh,
            &final_id_token,
            Some(&account_id),
            last_refresh.as_deref(),
        )?;
        // mkdir + atomic-rename can stall on slow filesystems
        // (NFS, spinning disk); spawn_blocking keeps the tokio runtime
        // free instead of pinning a worker thread.
        let path_for_blocking = path.clone();
        tokio::task::spawn_blocking(move || -> std::io::Result<()> {
            if let Some(parent) = path_for_blocking.parent() {
                if !parent.exists() {
                    std::fs::create_dir_all(parent)?;
                }
            }
            codex_credentials::write_auth_json_atomic(&path_for_blocking, &auth_json_text)
        })
        .await
        .map_err(|join_err| {
            CodexCredentialError::Db(anyhow::anyhow!("auth.json write join failed: {join_err}"))
        })??;
        Ok(account_id)
    }
}
