//! Add-time forced refresh (Fix 2). Split out of
//! `codex_credentials_runtime.rs` to keep that file under the file length
//! budget.

use crate::io::codex_oauth_refresh::{self, RefreshError, RefreshedTokens};
use crate::io::credentials;

use super::codex_credentials_runtime::CodexCredentialError;

/// Outcome of forcing a refresh at add time. `store_codex_credential`
/// matches on this to decide what to persist and which warning (if any)
/// to surface.
#[derive(Debug)]
pub(super) enum ForceRefreshOutcome {
    /// A refresh chain rotated successfully — either the pasted token (the
    /// common case) or, when the paste looked stale against mando's own
    /// stored chain, the row's token. The caller persists these tokens
    /// before running the add-time usage probe (Fix 1).
    Rotated(RefreshedTokens),
    /// The pasted session looked older than mando's own stored chain
    /// (`pasted_is_stale`), and revalidating mando's chain failed
    /// transiently. The existing row is left untouched; the caller
    /// surfaces a `ValidationSkippedTransient` warning instead of
    /// overwriting a possibly-fresher stored session with an unvalidated
    /// older paste.
    KeepExistingRow {
        plan_type: Option<String>,
        reason: String,
    },
    /// Forcing a refresh of the pasted token failed transiently
    /// (network/5xx). The caller proceeds with the pasted tokens,
    /// unvalidated, and surfaces a `ValidationSkippedTransient` warning.
    ProceedUnvalidated { reason: String },
    /// Refresh failed permanently — nothing usable was consumed. The
    /// caller rejects the add.
    Dead(String),
}

/// Force a refresh before persisting a pasted Codex `auth.json` (Fix 2).
///
/// When `existing_id` is `Some` (an upsert — a credential row for this
/// account already exists), the whole decision runs under that row's
/// per-credential lock (see
/// [`codex_oauth_refresh::with_credential_refresh_lock`]) so it cannot race
/// a concurrent poll/pick refresh using the same stored token. The row is
/// re-read inside the lock so the staleness comparison below sees the
/// latest tokens, not a snapshot taken before the lock was acquired.
///
/// `pasted_last_refresh` is the pasted auth.json's own `last_refresh` field
/// (RFC3339), used by [`pasted_is_stale`] to decide whether the paste is
/// older than mando's already-stored chain. A brand-new insert
/// (`existing_id: None`) has no row to compare against, so it always
/// refreshes the pasted token directly.
#[tracing::instrument(skip(pool, pasted_refresh_token, pasted_last_refresh))]
pub(super) async fn force_refresh_for_add(
    pool: &sqlx::SqlitePool,
    existing_id: Option<i64>,
    pasted_refresh_token: &str,
    pasted_last_refresh: Option<&str>,
) -> Result<ForceRefreshOutcome, CodexCredentialError> {
    let Some(id) = existing_id else {
        return Ok(classify_pasted_refresh(
            codex_oauth_refresh::refresh(pasted_refresh_token).await,
        ));
    };

    codex_oauth_refresh::with_credential_refresh_lock(id, || async {
        let row = credentials::get_row_by_id(pool, id)
            .await?
            .ok_or(CodexCredentialError::NotFound(id))?;

        if pasted_is_stale(
            row.refresh_token.as_deref(),
            row.token_updated_at,
            pasted_refresh_token,
            pasted_last_refresh,
        ) {
            if let Some(row_refresh_token) = row.refresh_token.as_deref() {
                return Ok(
                    match codex_oauth_refresh::refresh(row_refresh_token).await {
                        Ok(rotated) => ForceRefreshOutcome::Rotated(rotated),
                        Err(RefreshError::Permanent(row_reason)) => {
                            // The owned chain is dead. Maybe the paste is a
                            // genuinely new session — try it before giving up.
                            match codex_oauth_refresh::refresh(pasted_refresh_token).await {
                                Ok(rotated_pasted) => ForceRefreshOutcome::Rotated(rotated_pasted),
                                Err(RefreshError::Permanent(pasted_reason)) => {
                                    ForceRefreshOutcome::Dead(pasted_reason)
                                }
                                Err(_transient) => ForceRefreshOutcome::Dead(row_reason),
                            }
                        }
                        Err(row_transient) => ForceRefreshOutcome::KeepExistingRow {
                            plan_type: row.plan_type.clone(),
                            reason: row_transient.to_string(),
                        },
                    },
                );
            }
        }

        Ok(classify_pasted_refresh(
            codex_oauth_refresh::refresh(pasted_refresh_token).await,
        ))
    })
    .await
}

fn classify_pasted_refresh(result: Result<RefreshedTokens, RefreshError>) -> ForceRefreshOutcome {
    match result {
        Ok(rotated) => ForceRefreshOutcome::Rotated(rotated),
        Err(RefreshError::Permanent(reason)) => ForceRefreshOutcome::Dead(reason),
        Err(transient) => ForceRefreshOutcome::ProceedUnvalidated {
            reason: transient.to_string(),
        },
    }
}

/// Decide whether a pasted `auth.json` is stale against mando's own stored
/// refresh-token chain for the same account. True when: the row has a
/// refresh_token, it differs from the pasted one (a genuinely different
/// capture, not a re-paste of the same file), and the row's
/// `token_updated_at` is newer than the pasted file's own `last_refresh` —
/// or the pasted `last_refresh` is missing/unparseable, which counts as
/// stale (no evidence the paste is fresher than mando's stored chain).
pub(super) fn pasted_is_stale(
    row_refresh_token: Option<&str>,
    row_token_updated_at: Option<i64>,
    pasted_refresh_token: &str,
    pasted_last_refresh: Option<&str>,
) -> bool {
    let Some(row_refresh_token) = row_refresh_token else {
        return false;
    };
    if row_refresh_token == pasted_refresh_token {
        return false;
    }
    let Some(row_updated_at) = row_token_updated_at else {
        return false;
    };
    match pasted_last_refresh.and_then(parse_rfc3339_epoch_secs) {
        Some(pasted_secs) => row_updated_at > pasted_secs,
        None => true,
    }
}

/// Parse an RFC3339 timestamp (e.g. `auth.json`'s `last_refresh` field) to
/// Unix seconds. Shared with `codex_credentials_runtime::sync_payload_is_stale`.
pub(super) fn parse_rfc3339_epoch_secs(value: &str) -> Option<i64> {
    time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339)
        .ok()
        .map(|dt| dt.unix_timestamp())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pasted_is_stale_false_when_row_has_no_refresh_token() {
        assert!(!pasted_is_stale(None, Some(1_000), "pasted-rt", None));
    }

    #[test]
    fn pasted_is_stale_false_when_tokens_match() {
        assert!(!pasted_is_stale(
            Some("same-rt"),
            Some(1_000),
            "same-rt",
            Some("2020-01-01T00:00:00Z")
        ));
    }

    #[test]
    fn pasted_is_stale_false_when_row_has_no_token_updated_at() {
        assert!(!pasted_is_stale(Some("row-rt"), None, "pasted-rt", None));
    }

    #[test]
    fn pasted_is_stale_true_when_pasted_last_refresh_missing() {
        assert!(pasted_is_stale(
            Some("row-rt"),
            Some(1_000),
            "pasted-rt",
            None
        ));
    }

    #[test]
    fn pasted_is_stale_true_when_pasted_last_refresh_unparseable() {
        assert!(pasted_is_stale(
            Some("row-rt"),
            Some(1_000),
            "pasted-rt",
            Some("not-a-date")
        ));
    }

    #[test]
    fn pasted_is_stale_true_when_row_newer_than_pasted() {
        // row token_updated_at = 2026-01-02T00:00:00Z, pasted last_refresh = 2026-01-01T00:00:00Z
        assert!(pasted_is_stale(
            Some("row-rt"),
            Some(1_767_312_000),
            "pasted-rt",
            Some("2026-01-01T00:00:00Z")
        ));
    }

    #[test]
    fn pasted_is_stale_false_when_pasted_newer_than_row() {
        assert!(!pasted_is_stale(
            Some("row-rt"),
            Some(1_767_225_600),
            "pasted-rt",
            Some("2026-01-02T00:00:00Z")
        ));
    }

    #[test]
    fn pasted_is_stale_false_when_pasted_equals_row_timestamp() {
        // Equal timestamps: row is not strictly newer, so not stale.
        assert!(!pasted_is_stale(
            Some("row-rt"),
            Some(1_767_225_600),
            "pasted-rt",
            Some("2026-01-01T00:00:00Z")
        ));
    }
}
