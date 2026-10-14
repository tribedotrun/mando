//! Explicit Codex credential pick by id or label. Split out of
//! `codex_credentials_runtime.rs` to keep that file under the length budget.

use tracing::warn;

use crate::io::codex_credentials;
use crate::io::credentials;

use super::codex_credentials_runtime::{CodexCredentialError, PickedCodexCredential};
use super::codex_pick_helpers;
use super::codex_pick_refresh::{refresh_codex_access_token_on_pick, PickRefreshOutcome};
use super::settings_runtime::SettingsRuntime;

#[derive(Clone, Copy)]
enum CodexPickPurpose {
    Explicit,
    Warmup,
}

impl CodexPickPurpose {
    fn allows(&self, row: &credentials::CredentialRow) -> bool {
        row.disabled_at.is_none() && (matches!(self, Self::Explicit) || row.cli_eligible)
    }
}

impl SettingsRuntime {
    /// Pick a specific Codex credential by id or label. Honors the caller's
    /// explicit choice even when the row is expired or rate-limited, but never
    /// selects a manually disabled credential. Automatic-selection eligibility
    /// does not restrict an explicit choice.
    #[tracing::instrument(skip(self))]
    pub async fn pick_codex_credential_explicit(
        &self,
        id: Option<i64>,
        label: Option<&str>,
    ) -> Result<Option<PickedCodexCredential>, CodexCredentialError> {
        self.pick_codex_for_purpose(id, label, CodexPickPurpose::Explicit)
            .await
    }

    async fn pick_codex_for_purpose(
        &self,
        id: Option<i64>,
        label: Option<&str>,
        purpose: CodexPickPurpose,
    ) -> Result<Option<PickedCodexCredential>, CodexCredentialError> {
        let resolved_id = self
            .resolve_credential_pick_id(id, label)
            .await
            .map_err(|e| CodexCredentialError::Db(anyhow::Error::msg(e.to_string())))?;
        let Some(resolved_id) = resolved_id else {
            return Ok(None);
        };
        let row = credentials::get_row_by_id(&self.db_pool, resolved_id).await?;
        let Some(row) = row else {
            return Ok(None);
        };
        if row.provider != "codex" {
            return Ok(None);
        }
        if !purpose.allows(&row) {
            return Ok(None);
        }
        let Some(account_id) = row.account_id.clone() else {
            return Ok(None);
        };
        self.materialize_codex_pick(resolved_id, row.access_token, account_id, purpose)
            .await
    }

    /// Materialize a specific Codex credential for a usage warm-up run.
    /// Same refresh-and-serialize path as an explicit pick, but does not
    /// stamp `last_picked_at`: a warm-up is not a checkout and must not
    /// push the credential down the load-balanced pick order.
    #[tracing::instrument(skip(self))]
    pub(super) async fn materialize_codex_credential_for_warmup(
        &self,
        id: i64,
    ) -> Result<Option<PickedCodexCredential>, CodexCredentialError> {
        let Some(row) = credentials::get_row_by_id(&self.db_pool, id).await? else {
            return Ok(None);
        };
        if row.provider != "codex" {
            return Err(CodexCredentialError::NotCodex);
        }
        let Some(account_id) = row.account_id.clone() else {
            return Err(CodexCredentialError::NoAccountId);
        };
        self.materialize_codex_pick(id, row.access_token, account_id, CodexPickPurpose::Warmup)
            .await
    }

    async fn materialize_codex_pick(
        &self,
        id: i64,
        access_token: String,
        account_id: String,
        purpose: CodexPickPurpose,
    ) -> Result<Option<PickedCodexCredential>, CodexCredentialError> {
        let row = credentials::get_row_by_id(&self.db_pool, id)
            .await?
            .ok_or(CodexCredentialError::NotFound(id))?;
        if row.provider != "codex" {
            return Err(CodexCredentialError::NotCodex);
        }
        if !purpose.allows(&row) {
            return Ok(None);
        }

        let final_access = match refresh_codex_access_token_on_pick(
            &self.db_pool,
            id,
            &row,
            access_token,
        )
        .await?
        {
            PickRefreshOutcome::Ready(access_token) => access_token,
            PickRefreshOutcome::Expired | PickRefreshOutcome::SkipTransient => {
                return Ok(None);
            }
        };

        let row = credentials::get_row_by_id(&self.db_pool, id)
            .await?
            .ok_or(CodexCredentialError::NotFound(id))?;
        if !purpose.allows(&row) {
            return Ok(None);
        }
        let Some(refresh_token) = row.refresh_token.as_deref() else {
            warn!(
                module = "settings",
                credential_id = id,
                "Codex credential missing refresh_token on pick; skipping"
            );
            return Ok(None);
        };
        let last_refresh = codex_pick_helpers::codex_last_refresh_rfc3339(row.token_updated_at)?;
        let auth_json = codex_credentials::serialize_auth_json(
            &final_access,
            refresh_token,
            row.id_token.as_deref(),
            Some(&account_id),
            Some(&last_refresh),
        )
        .map_err(|e| CodexCredentialError::Db(anyhow::Error::msg(e.to_string())))?;

        if !matches!(purpose, CodexPickPurpose::Warmup) {
            credentials::record_codex_pick(&self.db_pool, id).await?;
        }
        Ok(Some(PickedCodexCredential {
            id,
            label: row.label,
            access_token: final_access,
            account_id,
            auth_json,
        }))
    }
}
