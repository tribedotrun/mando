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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_from_token_updated_at_when_present() {
        // 2026-01-01T00:00:00Z
        let secs = 1_767_225_600;
        let formatted = codex_last_refresh_rfc3339(Some(secs)).expect("format must succeed");
        assert_eq!(formatted, "2026-01-01T00:00:00Z");
    }

    #[test]
    fn falls_back_to_now_when_null() {
        let before = time::OffsetDateTime::now_utc().unix_timestamp();
        let formatted = codex_last_refresh_rfc3339(None).expect("format must succeed");
        let parsed =
            time::OffsetDateTime::parse(&formatted, &time::format_description::well_known::Rfc3339)
                .expect("must parse as RFC3339");
        let after = time::OffsetDateTime::now_utc().unix_timestamp();
        assert!(parsed.unix_timestamp() >= before && parsed.unix_timestamp() <= after);
    }
}
