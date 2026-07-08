//! Codex-specific credential-runtime methods (validate-then-store, pick for
//! shell injection). Lives in its own module so `settings_runtime.rs` stays
//! under the file length limit.

use tracing::warn;

use api_types::CodexCredentialAddWarning;

use crate::io::codex_credentials;
use crate::io::codex_probe;
use crate::io::credentials;
use crate::io::usage_probe::ProbeError;

use super::codex_add_guardrails;
use super::codex_add_persist::{persist_codex_probe_side_effects, persist_codex_row};
use super::codex_add_refresh::{
    force_refresh_for_add, parse_rfc3339_epoch_secs, ForceRefreshOutcome,
};
use super::codex_pick_helpers;
use super::codex_pick_refresh::{refresh_codex_access_token_on_pick, PickRefreshOutcome};
use super::settings_runtime::SettingsRuntime;

/// Outcome of a successful Codex credential add. Includes the parsed
/// `account_id` and `plan_type` for the API response so the UI can show
/// them without an extra round-trip.
#[derive(Debug, Clone)]
pub struct StoredCodexCredential {
    pub id: i64,
    pub account_id: String,
    pub plan_type: Option<String>,
    /// Set when the add succeeded but something about the pasted session
    /// is worth surfacing to the user (see [`CodexCredentialAddWarning`]).
    pub warning: Option<CodexCredentialAddWarning>,
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

/// Outcome of a Codex credential pick attempt (Fix 5). `pick` is `None` when
/// no credential is usable right now. `newly_expired` lists every candidate
/// the walk marked expired along the way — pick candidates are pre-filtered
/// to non-expired rows, so each entry here is a genuine transition into
/// expired/auth-dead the caller should notify on (not a wire type: the HTTP
/// response shape stays identical to before this fix).
#[derive(Debug, Clone)]
pub struct CodexPickOutcome {
    pub pick: Option<PickedCodexCredential>,
    pub newly_expired: Vec<(i64, String)>,
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
    #[error("upstream reset credits probe failed: {0}")]
    ResetCredits(#[source] ProbeError),
    #[error("database error: {0}")]
    Db(#[from] anyhow::Error),
    #[error("refresh token permanently invalid (re-add the credential): {0}")]
    PermanentRefreshFailure(String),
    /// Fix 3 guardrail: the pasted `refresh_token` byte-equals the ambient
    /// `~/.codex/auth.json` refresh_token — this is the user's live
    /// personal Codex login session, not a separate pool credential.
    #[error(
        "this is your live personal Codex login session (~/.codex); capture a separate session instead"
    )]
    AmbientSessionConflict,
    #[error("not a Codex credential")]
    NotCodex,
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
    /// checks the pasted session against the ambient `~/.codex` login,
    /// force-refreshes the pasted tokens (mando takes sole ownership of the
    /// session's rotation chain from this point), runs one synchronous
    /// usage probe to seed a snapshot, and inserts or replaces the row.
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

        // Fix 3: ambient-session guardrails, compared BEFORE the fix-2
        // force-refresh below rotates the pasted refresh_token — after
        // rotation the byte comparison would never match.
        let ambient = codex_credentials::read_ambient_auth();
        let mut warning = codex_add_guardrails::check_ambient_session(
            ambient.as_ref(),
            &parsed.refresh_token,
            &account_id,
        )?;

        let pasted_last_refresh = parsed.last_refresh.clone();
        let mut access_token = parsed.access_token;
        let mut refresh_token = parsed.refresh_token;
        let mut id_token = parsed.id_token;
        // Refreshable ChatGPT OAuth creds must not store JWT exp as expires_at;
        // id_token exp is short-lived and pick/probe refresh handles rotation.
        let expires_at = None;

        // Fix 2: force a refresh before persisting, bypassing staleness
        // checks. This is add-time session-liveness validation: a revoked
        // session still carries a JWT access_token that looks valid for
        // days, but the refresh call hits the OAuth server directly and
        // fails immediately if the session is dead. See
        // `force_refresh_for_add` for the pasted-vs-stored-chain decision
        // (an upsert may prefer revalidating mando's own stored chain over
        // trusting a stale paste).
        match force_refresh_for_add(
            &self.db_pool,
            existing_id,
            &refresh_token,
            pasted_last_refresh.as_deref(),
        )
        .await?
        {
            ForceRefreshOutcome::Rotated(refreshed) => {
                access_token = refreshed.access_token;
                refresh_token = refreshed.refresh_token;
                id_token = refreshed.id_token;
                claims = claims_from_optional_id_token(id_token.as_deref())?;

                // Fix 1: persist the rotated tokens BEFORE the usage probe.
                // The presented refresh_token is single-use and was just
                // consumed by the rotation above, so a transient probe
                // failure here must not lose the only copy of the rotated
                // tokens — that would strand the account (the pasted token
                // is already dead, and nothing was saved to retry with).
                let token_updated_at = time::OffsetDateTime::now_utc().unix_timestamp();
                let id = persist_codex_row(
                    &self.db_pool,
                    existing_id,
                    label,
                    &access_token,
                    &refresh_token,
                    id_token.as_deref(),
                    &account_id,
                    claims.plan_type.as_deref(),
                    expires_at,
                    token_updated_at,
                )
                .await?;

                match codex_probe::probe(&access_token, Some(&account_id)).await {
                    Ok(outcome) => {
                        let plan_type = outcome
                            .plan_type
                            .clone()
                            .or_else(|| claims.plan_type.clone());
                        persist_codex_probe_side_effects(
                            &self.db_pool,
                            id,
                            &outcome,
                            plan_type.as_deref(),
                        )
                        .await;
                        Ok(StoredCodexCredential {
                            id,
                            account_id,
                            plan_type,
                            warning,
                        })
                    }
                    Err(err) => {
                        warn!(
                            module = "settings",
                            credential_id = id,
                            error = %err,
                            "Codex add-time usage probe failed after persisting rotated tokens; tokens are safe, the next poll tick will seed the usage snapshot"
                        );
                        if warning.is_none() {
                            warning = Some(CodexCredentialAddWarning::UsageProbeFailed {
                                message: format!(
                                    "credential saved, but could not fetch usage/plan info from OpenAI right now ({err}); it will be refreshed automatically within 10 minutes"
                                ),
                            });
                        }
                        Ok(StoredCodexCredential {
                            id,
                            account_id,
                            plan_type: claims.plan_type.clone(),
                            warning,
                        })
                    }
                }
            }
            ForceRefreshOutcome::KeepExistingRow { plan_type, reason } => {
                warn!(
                    module = "settings",
                    error = %reason,
                    "Codex add-time revalidation of the existing stored session failed transiently; keeping stored tokens unchanged"
                );
                if warning.is_none() {
                    warning = Some(CodexCredentialAddWarning::ValidationSkippedTransient {
                        message: format!(
                            "the pasted session looked older than mando's stored session for \
                             this account, so the existing stored session was kept as-is; it \
                             could not be revalidated with OpenAI right now ({reason})"
                        ),
                    });
                }
                let id = existing_id.ok_or_else(|| {
                    CodexCredentialError::Db(anyhow::anyhow!(
                        "force_refresh_for_add returned KeepExistingRow without an existing credential"
                    ))
                })?;
                Ok(StoredCodexCredential {
                    id,
                    account_id,
                    plan_type,
                    warning,
                })
            }
            ForceRefreshOutcome::ProceedUnvalidated { reason } => {
                warn!(
                    module = "settings",
                    error = %reason,
                    "Codex add-time forced refresh failed transiently; proceeding with pasted tokens unvalidated"
                );
                if warning.is_none() {
                    warning = Some(CodexCredentialAddWarning::ValidationSkippedTransient {
                        message: format!(
                            "could not validate the pasted session with OpenAI right now \
                             ({reason}); added with the pasted tokens as-is"
                        ),
                    });
                }

                // Old order: nothing was consumed by this branch, so probe
                // BEFORE persisting; if the probe also fails, reject the
                // add as before (`?` propagates ProbeError).
                let outcome = codex_probe::probe(&access_token, Some(&account_id)).await?;
                let plan_type = outcome
                    .plan_type
                    .clone()
                    .or_else(|| claims.plan_type.clone());
                // Fix 4: unvalidated pasted tokens keep the paste's own
                // age instead of being stamped fresh.
                let token_updated_at = pasted_last_refresh
                    .as_deref()
                    .and_then(parse_rfc3339_epoch_secs)
                    .unwrap_or_else(|| time::OffsetDateTime::now_utc().unix_timestamp());
                let id = persist_codex_row(
                    &self.db_pool,
                    existing_id,
                    label,
                    &access_token,
                    &refresh_token,
                    id_token.as_deref(),
                    &account_id,
                    plan_type.as_deref(),
                    expires_at,
                    token_updated_at,
                )
                .await?;
                persist_codex_probe_side_effects(&self.db_pool, id, &outcome, plan_type.as_deref())
                    .await;
                Ok(StoredCodexCredential {
                    id,
                    account_id,
                    plan_type,
                    warning,
                })
            }
            ForceRefreshOutcome::Dead(reason) => {
                Err(CodexCredentialError::PermanentRefreshFailure(reason))
            }
        }
    }

    /// Pick the best Codex credential, refresh stale or near-expired tokens,
    /// and return the access token + account id for per-process env
    /// injection. Never writes `~/.codex/auth.json`. Also reports every
    /// candidate the walk marked expired along the way (Fix 5) so the
    /// caller can notify on each genuine expired/auth-dead transition.
    #[tracing::instrument(skip(self))]
    pub async fn pick_codex_credential(&self) -> Result<CodexPickOutcome, CodexCredentialError> {
        let mut newly_expired = Vec::new();
        for (id, access_token, account_id) in
            credentials::pick_for_codex_candidates(&self.db_pool).await?
        {
            let row = credentials::get_row_by_id(&self.db_pool, id)
                .await?
                .ok_or(CodexCredentialError::NotFound(id))?;
            if row.provider != "codex" {
                return Err(CodexCredentialError::NotCodex);
            }
            if row.disabled_at.is_some() {
                continue;
            }

            let final_access =
                match refresh_codex_access_token_on_pick(&self.db_pool, id, &row, access_token)
                    .await?
                {
                    PickRefreshOutcome::Ready(access_token) => access_token,
                    PickRefreshOutcome::Expired => {
                        newly_expired.push((id, row.label.clone()));
                        continue;
                    }
                    PickRefreshOutcome::SkipTransient => continue,
                };

            let row = credentials::get_row_by_id(&self.db_pool, id)
                .await?
                .ok_or(CodexCredentialError::NotFound(id))?;
            let Some(refresh_token) = row.refresh_token.as_deref() else {
                warn!(
                    module = "settings",
                    credential_id = id,
                    "Codex credential missing refresh_token on pick; skipping"
                );
                continue;
            };
            let last_refresh =
                codex_pick_helpers::codex_last_refresh_rfc3339(row.token_updated_at)?;
            let auth_json = codex_credentials::serialize_auth_json(
                &final_access,
                refresh_token,
                row.id_token.as_deref(),
                Some(&account_id),
                Some(&last_refresh),
            )
            .map_err(|e| CodexCredentialError::Db(anyhow::Error::msg(e.to_string())))?;

            credentials::record_codex_pick(&self.db_pool, id).await?;
            return Ok(CodexPickOutcome {
                pick: Some(PickedCodexCredential {
                    id,
                    label: row.label,
                    access_token: final_access,
                    account_id,
                    auth_json,
                }),
                newly_expired,
            });
        }
        Ok(CodexPickOutcome {
            pick: None,
            newly_expired,
        })
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

fn sync_payload_is_stale(row: &credentials::CredentialRow, last_refresh: Option<&str>) -> bool {
    let Some(stored) = row.token_updated_at else {
        return false;
    };
    let Some(incoming) = last_refresh.and_then(parse_rfc3339_epoch_secs) else {
        return false;
    };
    incoming < stored
}
