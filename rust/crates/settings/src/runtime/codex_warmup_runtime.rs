//! Runtime method for warming up one Codex credential's usage clock.
//!
//! Orchestrates pick-for-warm-up (token refresh + `auth.json`
//! serialization, without stamping `last_picked_at`), the throwaway
//! `codex exec` run, the token sync-back that a mid-run rotation requires,
//! and the `codex_warmup_at` stamp the poll policy keys on. Reachable from
//! the captain usage poller (automatic) and the HTTP route (manual).

use tracing::{info, warn};

use crate::io::codex_credentials;
use crate::io::codex_warmup;

use super::codex_credentials_runtime::CodexCredentialError;
use super::settings_runtime::SettingsRuntime;

/// Outcome of a completed warm-up.
#[derive(Debug, Clone)]
pub struct CodexWarmupReport {
    pub id: i64,
    pub label: String,
    /// Unix seconds when the warm-up finished; also persisted on the row.
    pub warmed_at: i64,
    /// Model the prompt ran on (always [`codex_warmup::WARMUP_MODEL`]).
    pub model: Option<String>,
    pub last_message: Option<String>,
    pub elapsed_ms: u64,
    /// True when Codex rotated the OAuth tokens during the run and the
    /// rotated pair was synced back into the credential row.
    pub tokens_rotated: bool,
}

impl SettingsRuntime {
    /// Fire one throwaway prompt on credential `id` so its rolling usage
    /// windows start counting now. Refuses disabled rows and rows whose
    /// token refresh fails (`NotUsable`); never touches `~/.codex`.
    #[tracing::instrument(skip(self))]
    pub async fn warm_codex_credential(
        &self,
        id: i64,
    ) -> Result<CodexWarmupReport, CodexCredentialError> {
        let picked = self
            .materialize_codex_credential_for_warmup(id)
            .await?
            .ok_or(CodexCredentialError::NotUsable(id))?;
        let model = Some(codex_warmup::WARMUP_MODEL.to_string());

        let outcome = codex_warmup::run_codex_warmup(&picked.auth_json).await?;

        let tokens_rotated = match outcome.rotated_auth_json.as_deref() {
            Some(rotated) => {
                // The refresh token that produced this rotation is now spent;
                // losing the rotated pair here would strand the account, so a
                // sync failure is a hard error and is surfaced as such.
                self.sync_codex_credential(id, rotated).await?;
                info!(
                    module = "settings",
                    credential_id = id,
                    label = %picked.label,
                    "Codex warm-up rotated tokens; synced back into the credential row"
                );
                true
            }
            None => false,
        };

        let warmed_at = time::OffsetDateTime::now_utc().unix_timestamp();
        let stamped = codex_credentials::record_codex_warmup(&self.db_pool, id, warmed_at).await?;
        if !stamped {
            warn!(
                module = "settings",
                credential_id = id,
                "Codex warm-up ran but the credential row vanished before the stamp"
            );
            return Err(CodexCredentialError::NotFound(id));
        }
        let elapsed_ms = u64::try_from(outcome.elapsed.as_millis()).unwrap_or(u64::MAX);
        info!(
            module = "settings",
            credential_id = id,
            label = %picked.label,
            model = model.as_deref().unwrap_or("default"),
            elapsed_ms,
            tokens_rotated,
            last_message = outcome.last_message.as_deref().unwrap_or(""),
            "Codex usage warm-up completed; rate-limit clock started"
        );
        Ok(CodexWarmupReport {
            id,
            label: picked.label,
            warmed_at,
            model,
            last_message: outcome.last_message,
            elapsed_ms,
            tokens_rotated,
        })
    }
}
