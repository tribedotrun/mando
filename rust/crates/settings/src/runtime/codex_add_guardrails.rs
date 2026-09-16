//! Add-time Codex credential guardrails (Fix 3). Split out of
//! `codex_credentials_runtime.rs` to keep that file under the length
//! budget; pure logic here is unit-testable without a DB or network call.
//!
//! Compares a pasted Codex `auth.json` session against the ambient
//! (non-pool) `~/.codex/auth.json` login — the ordinary local Codex
//! CLI/desktop login a developer uses day to day — to catch two mistakes:
//!
//! - Pasting the ambient session itself (the ambient `refresh_token`
//!   byte-equals the pasted one): this isn't a separate pool credential,
//!   it's the user's live personal login, and both mando and the personal
//!   session would fight over the same single-use rotating refresh token.
//! - Pasting a *different* session for the *same* account as the ambient
//!   login: not a mistake, but pool usage will share that account's rate
//!   limits with personal use, which is worth a warning.

use api_types::CodexCredentialAddWarning;

use crate::io::codex_credentials::ParsedCodexAuth;

use super::codex_credentials_runtime::CodexCredentialError;

/// Fix 3 ambient-session guardrails. `ambient` is `None` when the ambient
/// `~/.codex/auth.json` is missing, unreadable, or fails to parse (a
/// best-effort check with no hard dependency on ambient state).
///
/// - `Err(AmbientSessionConflict)` — the pasted `refresh_token` byte-equals
///   the ambient session's: this is the user's live personal Codex login,
///   not a separate pool credential.
/// - `Ok(Some(warning))` — the pasted `account_id` matches the ambient
///   account but the tokens differ (a separate session for the same
///   account): the add succeeds, but pool usage will share that account's
///   rate limits with the user's personal session.
/// - `Ok(None)` — no ambient conflict.
pub(super) fn check_ambient_session(
    ambient: Option<&ParsedCodexAuth>,
    pasted_refresh_token: &str,
    pasted_account_id: &str,
) -> Result<Option<CodexCredentialAddWarning>, CodexCredentialError> {
    let Some(ambient) = ambient else {
        return Ok(None);
    };
    if ambient.refresh_token == pasted_refresh_token {
        return Err(CodexCredentialError::AmbientSessionConflict);
    }
    if ambient.account_id.as_deref() == Some(pasted_account_id) {
        return Ok(Some(CodexCredentialAddWarning::SharedAccountWithAmbient {
            message: format!(
                "account {pasted_account_id} is also logged in at ~/.codex; pool usage will share this account's rate limits with your personal Codex session"
            ),
        }));
    }
    Ok(None)
}
