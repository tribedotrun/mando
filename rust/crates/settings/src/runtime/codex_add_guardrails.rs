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

#[cfg(test)]
mod tests {
    use super::*;

    fn ambient_auth(refresh_token: &str, account_id: Option<&str>) -> ParsedCodexAuth {
        ParsedCodexAuth {
            access_token: "ambient-access".to_string(),
            refresh_token: refresh_token.to_string(),
            id_token: None,
            account_id: account_id.map(|s| s.to_string()),
            last_refresh: None,
        }
    }

    #[test]
    fn missing_ambient_file_skips_check() {
        let result = check_ambient_session(None, "pasted-rt", "acct-1");
        assert!(matches!(result, Ok(None)));
    }

    #[test]
    fn matching_refresh_token_is_hard_conflict() {
        let ambient = ambient_auth("shared-rt", Some("acct-1"));
        let err = check_ambient_session(Some(&ambient), "shared-rt", "acct-1")
            .expect_err("byte-equal refresh_token must hard-error");
        assert!(matches!(err, CodexCredentialError::AmbientSessionConflict));
    }

    #[test]
    fn matching_account_different_token_is_soft_warning() {
        let ambient = ambient_auth("ambient-rt", Some("acct-1"));
        let warning = check_ambient_session(Some(&ambient), "pasted-rt", "acct-1")
            .expect("same account, different token must not hard-error");
        assert!(matches!(
            warning,
            Some(CodexCredentialAddWarning::SharedAccountWithAmbient { .. })
        ));
    }

    #[test]
    fn different_account_and_token_is_clean() {
        let ambient = ambient_auth("ambient-rt", Some("acct-other"));
        let result = check_ambient_session(Some(&ambient), "pasted-rt", "acct-1")
            .expect("unrelated ambient session must not error");
        assert!(result.is_none());
    }

    #[test]
    fn ambient_without_account_id_produces_no_warning() {
        let ambient = ambient_auth("ambient-rt", None);
        let result = check_ambient_session(Some(&ambient), "pasted-rt", "acct-1")
            .expect("ambient without account_id must not error");
        assert!(result.is_none());
    }
}
