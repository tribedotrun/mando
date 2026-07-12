//! Row-scoped Codex `auth.json` update: replace the stored session on an
//! existing credential with a freshly captured one for the SAME ChatGPT
//! account. Guards the row identity up front, then delegates to the shared
//! [`SettingsRuntime::store_codex_credential`] validate-then-store pipeline
//! (which upserts the same row by `account_id` and keeps the row's label).
//!
//! Lives in its own module so `codex_credentials_runtime.rs` stays under
//! the file-length limit.

use crate::io::codex_credentials;
use crate::io::credentials;

use super::codex_credentials_runtime::{
    claims_from_optional_id_token, CodexCredentialError, StoredCodexCredential,
};
use super::settings_runtime::SettingsRuntime;

impl SettingsRuntime {
    /// Replace the auth session on Codex credential `id` with the pasted
    /// `auth_json`. The pasted session must belong to the same ChatGPT
    /// account as the stored row ([`CodexCredentialError::AccountMismatch`]
    /// otherwise — a different account is an add, not an update). On match,
    /// runs the full add pipeline against the row's existing label, which
    /// force-refreshes, upserts the same row by `account_id`, and probes
    /// usage. Returns the row's label alongside the stored outcome.
    #[tracing::instrument(skip(self, auth_json_text))]
    pub async fn update_codex_credential_auth(
        &self,
        id: i64,
        auth_json_text: &str,
    ) -> Result<(String, StoredCodexCredential), CodexCredentialError> {
        let row = credentials::get_row_by_id(&self.db_pool, id)
            .await?
            .ok_or(CodexCredentialError::NotFound(id))?;
        if row.provider != "codex" {
            return Err(CodexCredentialError::NotCodex);
        }

        let parsed = codex_credentials::parse_auth_json(auth_json_text)?;
        let claims = claims_from_optional_id_token(parsed.id_token.as_deref())?;
        let pasted_account_id = parsed
            .account_id
            .clone()
            .or_else(|| claims.account_id.clone())
            .ok_or(CodexCredentialError::NoAccountId)?;

        // A codex row without an account_id is malformed (insert always
        // sets one); surface that as NoAccountId rather than an
        // AccountMismatch with an empty `expected`.
        let row_account_id = row
            .account_id
            .clone()
            .ok_or(CodexCredentialError::NoAccountId)?;
        if row_account_id != pasted_account_id {
            return Err(CodexCredentialError::AccountMismatch {
                expected: row_account_id,
                got: pasted_account_id,
            });
        }

        let stored = self
            .store_codex_credential(&row.label, auth_json_text)
            .await?;
        Ok((row.label, stored))
    }
}
