//! Pure decision layer for the Codex "sign in with browser" flow: capture
//! ownership, row-scoped account guards, label resolution, and user-facing
//! failure messages. No I/O and no flow state — the state machine lives in
//! `codex_login_runtime.rs`. Split out to keep that file under the
//! file-length limit.

use api_types::CodexLoginStatus;

use crate::io::codex_credentials;

use super::codex_credentials_runtime::CodexCredentialError;

/// Failure message stored on a row-scoped flow when the browser sign-in
/// captured a session for a different ChatGPT account than the target row.
pub(super) const ROW_LOGIN_ACCOUNT_MISMATCH: &str =
    "signed-in ChatGPT account does not match this credential; use Add account for a new account";

/// Failure message stored on a row-scoped flow when the target credential
/// row was deleted or changed while the browser flow was pending.
pub(super) const ROW_LOGIN_TARGET_CHANGED: &str =
    "credential was removed or changed during sign-in; use Add account instead";

/// Identity extracted from a captured `auth.json`: the ChatGPT account it
/// belongs to plus the optional id_token `email` claim (label fallback).
pub(super) struct CapturedIdentity {
    pub(super) account_id: String,
    pub(super) email: Option<String>,
}

pub(super) fn captured_identity(auth_json: &str) -> Result<CapturedIdentity, CodexCredentialError> {
    let parsed = codex_credentials::parse_auth_json(auth_json)?;
    let claims = match parsed.id_token.as_deref() {
        Some(token) => Some(codex_credentials::decode_id_token_claims(token)?),
        None => None,
    };
    let account_id = parsed
        .account_id
        .clone()
        .or_else(|| claims.as_ref().and_then(|c| c.account_id.clone()))
        .ok_or(CodexCredentialError::NoAccountId)?;
    let email = claims.and_then(|c| c.email);
    Ok(CapturedIdentity { account_id, email })
}

/// Pure row-scoped guard: `true` when an expected account is set and the
/// captured session belongs to a different one (flow must fail without
/// storing). `false` when no expectation exists or the accounts match.
pub(super) fn captured_account_mismatches(
    expected_account_id: Option<&str>,
    captured_account_id: &str,
) -> bool {
    expected_account_id.is_some_and(|expected| expected != captured_account_id)
}

/// Pure ownership guard for a finished capture. The background task can
/// hold a successful capture for a flow that was cancelled-and-replaced
/// while the child was exiting; only the flow that is still the CURRENT
/// slot entry (same `login_id`) and still `Pending` owns its capture —
/// anything else must be dropped without storing. `current` is the slot's
/// `(login_id, status)` pair, `None` when the slot is empty.
pub(super) fn flow_owns_capture(current: Option<(&str, CodexLoginStatus)>, login_id: &str) -> bool {
    current.is_some_and(|(current_id, status)| {
        current_id == login_id && status == CodexLoginStatus::Pending
    })
}

/// Pure row-scoped guard re-checked just before storing: `true` only when
/// the re-fetched target row still exists as a Codex row for the captured
/// account. `row` is the re-fetched `(provider, account_id)` pair, `None`
/// when the row is gone. A deleted or repurposed row must NOT be
/// resurrected by the store's account-keyed upsert (which would INSERT a
/// fresh row under the old label).
pub(super) fn target_row_intact(
    row: Option<(&str, Option<&str>)>,
    captured_account_id: &str,
) -> bool {
    row.is_some_and(|(provider, account_id)| {
        provider == "codex" && account_id == Some(captured_account_id)
    })
}

/// Pure resolution of the label to store a captured Codex login under.
/// Order: (0) the target row's label for a row-scoped re-login, (1) a
/// non-empty requested label, (2) the existing credential row's label when
/// this account was already stored (a re-login keeps its label), (3) the
/// id_token `email` claim, (4) `codex-<first 8 chars of account_id>`.
pub(super) fn resolve_codex_login_label(
    target_row_label: Option<&str>,
    requested_label: Option<&str>,
    existing_label: Option<&str>,
    email_claim: Option<&str>,
    account_id: &str,
) -> String {
    if let Some(label) = target_row_label.map(str::trim).filter(|l| !l.is_empty()) {
        return label.to_string();
    }
    if let Some(label) = requested_label.map(str::trim).filter(|l| !l.is_empty()) {
        return label.to_string();
    }
    if let Some(label) = existing_label.map(str::trim).filter(|l| !l.is_empty()) {
        return label.to_string();
    }
    if let Some(email) = email_claim.map(str::trim).filter(|e| !e.is_empty()) {
        return email.to_string();
    }
    // `.chars().take(8)` (not a byte slice) so a non-ASCII account_id can
    // never panic on a mid-character split.
    let short_id: String = account_id.chars().take(8).collect();
    format!("codex-{short_id}")
}

/// Map a [`CodexCredentialError`] to the message stored on a `Failed` flow,
/// reusing the add-endpoint's phrasing where practical
/// (`routes_credentials_codex::add_codex_credential`) and otherwise
/// passing the error's `Display` through as-is.
pub(super) fn codex_login_store_error_message(err: &CodexCredentialError) -> String {
    match err {
        CodexCredentialError::AmbientSessionConflict => {
            "captured session matches your live personal Codex login (~/.codex); this should \
             not happen for a fresh browser sign-in. Please retry."
                .to_string()
        }
        CodexCredentialError::PermanentRefreshFailure(reason) => {
            format!("captured session is invalid or revoked ({reason}); please retry sign in with browser")
        }
        CodexCredentialError::DuplicateLabel(label, _) => {
            format!(
                "another credential already uses the label {label:?}; remove it or add this \
                 account manually with a different label"
            )
        }
        other => other.to_string(),
    }
}

pub(super) fn panic_to_string(panic: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = panic.downcast_ref::<&'static str>() {
        format!("panic: {s}")
    } else if let Some(s) = panic.downcast_ref::<String>() {
        format!("panic: {s}")
    } else {
        "panic: (unknown payload)".to_string()
    }
}
