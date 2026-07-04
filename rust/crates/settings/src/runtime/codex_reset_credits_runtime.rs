//! Runtime method for reading Codex reset credits for one stored credential.

use crate::io::codex_reset_credits::{self, CodexResetCreditsOutcome};
use crate::io::credentials;
use crate::io::provider_probe;
use crate::io::usage_probe::ProbeError;

use super::codex_credentials_runtime::CodexCredentialError;
use super::settings_runtime::SettingsRuntime;

impl SettingsRuntime {
    /// Read available Codex reset credits for a credential. On a stale access
    /// token, refresh via the existing Codex usage-probe path, then retry once.
    #[tracing::instrument(skip(self))]
    pub async fn codex_reset_credits(
        &self,
        id: i64,
    ) -> Result<CodexResetCreditsOutcome, CodexCredentialError> {
        let row = self.codex_row(id).await?;
        let account_id = row
            .account_id
            .as_deref()
            .ok_or(CodexCredentialError::NoAccountId)?;

        match codex_reset_credits::fetch(&row.access_token, account_id).await {
            Ok(outcome) => Ok(outcome),
            Err(ProbeError::Unauthorized) => {
                provider_probe::probe(&self.db_pool, &row)
                    .await
                    .map_err(CodexCredentialError::Probe)?;
                let refreshed = self.codex_row(id).await?;
                let refreshed_account_id = refreshed
                    .account_id
                    .as_deref()
                    .ok_or(CodexCredentialError::NoAccountId)?;
                codex_reset_credits::fetch(&refreshed.access_token, refreshed_account_id)
                    .await
                    .map_err(CodexCredentialError::ResetCredits)
            }
            Err(err) => Err(CodexCredentialError::ResetCredits(err)),
        }
    }

    async fn codex_row(&self, id: i64) -> Result<credentials::CredentialRow, CodexCredentialError> {
        let row = credentials::get_row_by_id(&self.db_pool, id)
            .await?
            .ok_or(CodexCredentialError::NotFound(id))?;
        if row.provider != "codex" {
            return Err(CodexCredentialError::NotCodex);
        }
        Ok(row)
    }
}
