//! Codex-specific credential-runtime methods (validate-then-store, pick for
//! shell injection). Lives in its own module so `settings_runtime.rs` stays
//! under the file length limit.

use tracing::{debug, warn};

use global_types::RateLimitStatus;

use crate::io::cc_failover;
use crate::io::codex_credentials;
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

/// Credential picked for per-process env injection (never writes auth.json).
#[derive(Debug, Clone)]
pub struct PickedCodexCredential {
    pub id: i64,
    pub label: String,
    pub access_token: String,
    pub account_id: String,
    pub auth_json: String,
}

#[derive(Debug, thiserror::Error)]
pub enum CodexCredentialError {
    #[error("auth.json invalid: {0}")]
    AuthJson(#[from] codex_credentials::AuthJsonError),
    #[error("auth.json is missing tokens.account_id (no chatgpt_account_id JWT claim either)")]
    NoAccountId,
    #[error("a Codex credential for account {0} already exists (id={1})")]
    DuplicateAccount(String, i64),
    #[error("credential label {0:?} already exists (id={1})")]
    DuplicateLabel(String, i64),
    #[error("upstream usage probe failed: {0}")]
    Probe(#[from] ProbeError),
    #[error("database error: {0}")]
    Db(#[from] anyhow::Error),
    #[error("refresh token permanently invalid (re-add the credential): {0}")]
    PermanentRefreshFailure(String),
    #[error("not a Codex credential")]
    NotCodex,
    #[error("codex credential is missing the required token fields")]
    MissingTokens,
    #[error("credential id={0} not found")]
    NotFound(i64),
    #[error("auth.json account_id {got} does not match stored credential ({expected})")]
    AccountMismatch { expected: String, got: String },
    #[error("failed to persist refreshed tokens: {0}")]
    TokenPersistFailed(String),
}

impl SettingsRuntime {
    /// Validate-then-store a Codex `auth.json` blob. Parses the file,
    /// rejects non-chatgpt mode, decodes the JWT to extract `plan_type` +
    /// `account_id` (with fallback to the file's `tokens.account_id`),
    /// refreshes stale pasted auth when possible, runs one synchronous usage
    /// probe to seed a snapshot, and inserts or replaces the row.
    #[tracing::instrument(skip(self, auth_json_text))]
    pub async fn store_codex_credential(
        &self,
        label: &str,
        auth_json_text: &str,
    ) -> Result<StoredCodexCredential, CodexCredentialError> {
        let parsed = codex_credentials::parse_auth_json(auth_json_text)?;
        let mut claims = claims_from_optional_id_token(parsed.id_token.as_deref())?;
        let account_id = parsed
            .account_id
            .clone()
            .or_else(|| claims.account_id.clone())
            .ok_or(CodexCredentialError::NoAccountId)?;

        let existing_id =
            codex_credentials::find_codex_id_by_account(&self.db_pool, &account_id).await?;
        if let Some(label_owner) = credentials::find_by_label(&self.db_pool, label).await? {
            if existing_id != Some(label_owner) {
                return Err(CodexCredentialError::DuplicateLabel(
                    label.to_string(),
                    label_owner,
                ));
            }
        }

        let mut access_token = parsed.access_token;
        let mut refresh_token = parsed.refresh_token;
        let mut id_token = parsed.id_token;
        let outcome = match codex_probe::probe(&access_token, Some(&account_id)).await {
            Ok(outcome) => outcome,
            Err(ProbeError::Unauthorized) => {
                let refreshed = refresh_for_add(&refresh_token).await?;
                access_token = refreshed.access_token;
                refresh_token = refreshed.refresh_token;
                id_token = refreshed.id_token;
                claims = claims_from_optional_id_token(id_token.as_deref())?;
                codex_probe::probe(&access_token, Some(&account_id)).await?
            }
            Err(err) => return Err(CodexCredentialError::Probe(err)),
        };
        let plan_type = outcome.plan_type.clone().or(claims.plan_type);
        // Refreshable ChatGPT OAuth creds must not store JWT exp as expires_at;
        // id_token exp is short-lived and pick/probe refresh handles rotation.
        let expires_at = None;

        let id = if let Some(existing_id) = existing_id {
            let updated = codex_credentials::replace_codex(
                &self.db_pool,
                existing_id,
                label,
                &access_token,
                &refresh_token,
                id_token.as_deref(),
                &account_id,
                plan_type.as_deref(),
                expires_at,
            )
            .await?;
            if !updated {
                return Err(CodexCredentialError::NotFound(existing_id));
            }
            existing_id
        } else {
            codex_credentials::insert_codex(
                &self.db_pool,
                label,
                &access_token,
                &refresh_token,
                id_token.as_deref(),
                &account_id,
                plan_type.as_deref(),
                expires_at,
            )
            .await?
        };

        persist_codex_probe_side_effects(&self.db_pool, id, &outcome, plan_type.as_deref()).await;

        Ok(StoredCodexCredential {
            id,
            account_id,
            plan_type,
        })
    }

    /// Pick the best Codex credential, refresh tokens when stale, and return
    /// the access token + account id for per-process env injection. Never
    /// writes `~/.codex/auth.json`.
    #[tracing::instrument(skip(self))]
    pub async fn pick_codex_credential(
        &self,
    ) -> Result<Option<PickedCodexCredential>, CodexCredentialError> {
        let Some((id, access_token, account_id)) =
            credentials::pick_for_codex(&self.db_pool).await?
        else {
            return Ok(None);
        };

        credentials::record_codex_pick(&self.db_pool, id).await?;

        let row = credentials::get_row_by_id(&self.db_pool, id)
            .await?
            .ok_or(CodexCredentialError::NotFound(id))?;
        if row.provider != "codex" {
            return Err(CodexCredentialError::NotCodex);
        }

        let final_access =
            refresh_codex_access_token_on_pick(&self.db_pool, id, &row, access_token).await?;

        let row = credentials::get_row_by_id(&self.db_pool, id)
            .await?
            .ok_or(CodexCredentialError::NotFound(id))?;

        let refresh_token = row
            .refresh_token
            .as_deref()
            .ok_or(CodexCredentialError::MissingTokens)?;
        let last_refresh = time::OffsetDateTime::now_utc()
            .format(&time::format_description::well_known::Rfc3339)
            .map_err(|e| CodexCredentialError::Db(anyhow::Error::msg(e.to_string())))?;
        let auth_json = codex_credentials::serialize_auth_json(
            &final_access,
            refresh_token,
            row.id_token.as_deref(),
            Some(&account_id),
            Some(&last_refresh),
        )
        .map_err(|e| CodexCredentialError::Db(anyhow::Error::msg(e.to_string())))?;

        let labels = credentials::labels_by_ids(&self.db_pool, &[id]).await?;
        let label = labels.get(&id).cloned().unwrap_or_else(|| id.to_string());

        Ok(Some(PickedCodexCredential {
            id,
            label,
            access_token: final_access,
            account_id,
            auth_json,
        }))
    }

    /// Persist refreshed tokens from a per-process `CODEX_HOME/auth.json` back
    /// into the credential row. Called on shell exit after Codex may have
    /// rotated tokens in the temp home.
    #[tracing::instrument(skip(self, auth_json_text))]
    pub async fn sync_codex_credential(
        &self,
        id: i64,
        auth_json_text: &str,
    ) -> Result<(), CodexCredentialError> {
        let parsed = codex_credentials::parse_auth_json(auth_json_text)?;
        let claims = claims_from_optional_id_token(parsed.id_token.as_deref())?;
        let account_id = parsed
            .account_id
            .clone()
            .or_else(|| claims.account_id.clone())
            .ok_or(CodexCredentialError::NoAccountId)?;

        let row = credentials::get_row_by_id(&self.db_pool, id)
            .await?
            .ok_or(CodexCredentialError::NotFound(id))?;
        if row.provider != "codex" {
            return Err(CodexCredentialError::NotCodex);
        }
        if row.account_id.as_deref() != Some(account_id.as_str()) {
            return Err(CodexCredentialError::AccountMismatch {
                expected: row.account_id.unwrap_or_default(),
                got: account_id,
            });
        }

        if parsed.access_token == row.access_token
            && row
                .refresh_token
                .as_deref()
                .is_some_and(|stored| stored == parsed.refresh_token)
        {
            return Ok(());
        }

        if sync_payload_is_stale(&row, parsed.last_refresh.as_deref()) {
            warn!(
                module = "settings",
                credential_id = id,
                incoming_last_refresh = parsed.last_refresh.as_deref().unwrap_or("missing"),
                stored_token_updated_at = row.token_updated_at.unwrap_or_default(),
                "skipping stale Codex temp auth sync"
            );
            return Ok(());
        }

        let updated = codex_credentials::update_codex_tokens(
            &self.db_pool,
            id,
            &parsed.access_token,
            &parsed.refresh_token,
            parsed.id_token.as_deref(),
            None,
        )
        .await?;
        if !updated {
            return Err(CodexCredentialError::NotFound(id));
        }
        Ok(())
    }
}

fn claims_from_optional_id_token(
    id_token: Option<&str>,
) -> Result<codex_credentials::CodexJwtClaims, CodexCredentialError> {
    match id_token {
        Some(token) => codex_credentials::decode_id_token_claims(token).map_err(Into::into),
        None => Ok(codex_credentials::CodexJwtClaims {
            plan_type: None,
            account_id: None,
            exp: None,
        }),
    }
}

async fn refresh_for_add(
    refresh_token: &str,
) -> Result<codex_oauth_refresh::RefreshedTokens, CodexCredentialError> {
    codex_oauth_refresh::refresh(refresh_token)
        .await
        .map_err(refresh_error_to_credential_error)
}

fn refresh_error_to_credential_error(
    err: codex_oauth_refresh::RefreshError,
) -> CodexCredentialError {
    match err {
        codex_oauth_refresh::RefreshError::Permanent(reason) => {
            CodexCredentialError::PermanentRefreshFailure(reason)
        }
        codex_oauth_refresh::RefreshError::Unauthorized => CodexCredentialError::Probe(
            ProbeError::Network("Codex OAuth refresh returned 401".to_string()),
        ),
        codex_oauth_refresh::RefreshError::Http { status, .. } => {
            CodexCredentialError::Probe(ProbeError::Http(status))
        }
        codex_oauth_refresh::RefreshError::Network(msg) => {
            CodexCredentialError::Probe(ProbeError::Network(msg))
        }
        codex_oauth_refresh::RefreshError::Parse(msg) => {
            CodexCredentialError::Probe(ProbeError::Parse(msg))
        }
    }
}

async fn persist_codex_probe_side_effects(
    pool: &sqlx::SqlitePool,
    id: i64,
    outcome: &CodexProbeOutcome,
    plan_type: Option<&str>,
) {
    global_infra::best_effort!(
        credentials::set_usage_snapshot(pool, id, &outcome.snapshot).await,
        "codex_credentials_runtime: set_usage_snapshot on add"
    );
    global_infra::best_effort!(
        codex_credentials::update_codex_plan_and_credits(
            pool,
            id,
            plan_type,
            outcome.credits_balance.as_deref(),
            outcome.credits_unlimited,
        )
        .await,
        "codex_credentials_runtime: update_codex_plan_and_credits on add"
    );
    if matches!(outcome.snapshot.unified_status, RateLimitStatus::Rejected) {
        let reset_at = binding_reset_at(&outcome.snapshot).max(0) as u64;
        let until = cc_failover::compute_cooldown_until(
            time::OffsetDateTime::now_utc().unix_timestamp().max(0) as u64,
            Some(reset_at),
            outcome.snapshot.representative_claim.as_deref(),
        );
        global_infra::best_effort!(
            credentials::set_rate_limit_cooldown(pool, id, until as i64).await,
            "codex_credentials_runtime: set cooldown on rejected add probe"
        );
    }
}

fn binding_reset_at(snapshot: &crate::io::usage_probe::UsageSnapshot) -> i64 {
    match snapshot.representative_claim.as_deref() {
        Some("five_hour") => snapshot.five_hour.reset_at,
        Some(s) if s.starts_with("seven_day") => snapshot.seven_day.reset_at,
        _ => snapshot.five_hour.reset_at.max(snapshot.seven_day.reset_at),
    }
}

async fn refresh_codex_access_token_on_pick(
    pool: &sqlx::SqlitePool,
    id: i64,
    row: &credentials::CredentialRow,
    access_token: String,
) -> Result<String, CodexCredentialError> {
    if !codex_row_should_refresh(row) {
        return Ok(access_token);
    }

    codex_oauth_refresh::with_credential_refresh_lock(id, || async {
        let current = credentials::get_row_by_id(pool, id)
            .await?
            .ok_or(CodexCredentialError::NotFound(id))?;
        if current.provider != "codex" {
            return Err(CodexCredentialError::NotCodex);
        }
        if !codex_row_should_refresh(&current) {
            return Ok(current.access_token);
        }
        let Some(refresh_token) = current.refresh_token.as_deref() else {
            return Ok(current.access_token);
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
                Ok(refreshed.access_token)
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
                Err(CodexCredentialError::PermanentRefreshFailure(reason))
            }
            Err(err) => {
                warn!(
                    module = "settings",
                    credential_id = id,
                    error = %err,
                    "Codex refresh failed transiently on pick; using stored access_token"
                );
                Ok(current.access_token)
            }
        }
    })
    .await
}

fn codex_row_should_refresh(row: &credentials::CredentialRow) -> bool {
    let now_secs = time::OffsetDateTime::now_utc().unix_timestamp();
    let exp_secs = row.id_token.as_deref().and_then(|token| {
        match codex_credentials::decode_id_token_claims(token) {
            Ok(claims) => claims.exp,
            Err(err) => {
                debug!(
                    module = "settings",
                    credential_id = row.id,
                    error = %err,
                    "id_token decode failed; skipping proactive refresh exp check"
                );
                None
            }
        }
    });
    codex_oauth_refresh::should_refresh(
        exp_secs,
        row.token_updated_at.or(row.last_probed_at),
        now_secs,
    )
}

fn sync_payload_is_stale(row: &credentials::CredentialRow, last_refresh: Option<&str>) -> bool {
    let Some(stored) = row.token_updated_at else {
        return false;
    };
    let Some(incoming) = last_refresh.and_then(parse_rfc3339_epoch_secs) else {
        return false;
    };
    incoming < stored
}

fn parse_rfc3339_epoch_secs(value: &str) -> Option<i64> {
    time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339)
        .ok()
        .map(|dt| dt.unix_timestamp())
}
