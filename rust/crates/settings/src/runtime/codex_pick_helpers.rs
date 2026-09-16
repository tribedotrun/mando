//! Pure helpers for `pick_codex_credential`. Split out of
//! `codex_credentials_runtime.rs` to keep that file under the length
//! budget; pure logic here is unit-testable without a DB call.

use super::codex_credentials_runtime::CodexCredentialError;

/// RFC3339 `last_refresh` for a materialized pick `auth.json` (Fix 5a).
/// Derives from the row's `token_updated_at` (Unix seconds) when present,
/// falling back to now() only when `NULL`. Stamping pick-time now()
/// unconditionally (the prior behavior) would mask token age from Codex's
/// own client-side refresh heuristics, which read `last_refresh` to decide
/// whether to refresh proactively.
pub(super) fn codex_last_refresh_rfc3339(
    token_updated_at: Option<i64>,
) -> Result<String, CodexCredentialError> {
    let ts = match token_updated_at {
        Some(secs) => time::OffsetDateTime::from_unix_timestamp(secs)
            .map_err(|e| CodexCredentialError::Db(anyhow::Error::msg(e.to_string())))?,
        None => time::OffsetDateTime::now_utc(),
    };
    ts.format(&time::format_description::well_known::Rfc3339)
        .map_err(|e| CodexCredentialError::Db(anyhow::Error::msg(e.to_string())))
}
