//! When to fire a Codex usage warm-up (see `io::codex_warmup`).
//!
//! Pure so the poll loop's decision is unit-testable without a database.
//! Lives in the runtime tier because it reads `io` row and snapshot types.

use crate::io::credentials::CredentialRow;
use crate::io::usage_probe::{RateLimitStatus, UsageSnapshot};

/// Minimum spacing between warm-ups of one credential. A warm-up starts the
/// 5h window; a fresh probe inside that window still rounds to 0% (the
/// dummy prompt is far below one percent of the allowance), so the zero
/// reading alone cannot tell "window running" from "window reset". Waiting
/// the window length before trusting a zero reading again means each idle
/// credential is warmed once per reset cycle.
pub const CODEX_WARMUP_MIN_INTERVAL_SECS: i64 = 5 * 60 * 60;

/// True when `snapshot` (just probed for `row`) shows the credential idle
/// with no usage clock running, and no warm-up has run within the last
/// window length.
pub fn codex_warmup_due(row: &CredentialRow, snapshot: &UsageSnapshot, now_secs: i64) -> bool {
    if row.provider != "codex" || row.disabled_at.is_some() {
        return false;
    }
    if matches!(snapshot.unified_status, RateLimitStatus::Rejected) {
        return false;
    }
    if snapshot.five_hour.utilization > 0.0 || snapshot.seven_day.utilization > 0.0 {
        return false;
    }
    match row.codex_warmup_at {
        Some(last) => now_secs.saturating_sub(last) >= CODEX_WARMUP_MIN_INTERVAL_SECS,
        None => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::io::usage_probe::WindowState;

    fn row(provider: &str, warmup_at: Option<i64>) -> CredentialRow {
        CredentialRow {
            id: 1,
            label: "acct".into(),
            access_token: "tok".into(),
            expires_at: None,
            rate_limit_cooldown_until: None,
            disabled_at: None,
            cli_eligible: true,
            created_at: String::new(),
            updated_at: String::new(),
            five_hour_utilization: None,
            five_hour_reset_at: None,
            five_hour_status: None,
            seven_day_utilization: None,
            seven_day_reset_at: None,
            seven_day_status: None,
            seven_day_fable_utilization: None,
            seven_day_fable_reset_at: None,
            seven_day_fable_status: None,
            unified_status: None,
            representative_claim: None,
            last_probed_at: None,
            last_picked_at: None,
            token_updated_at: None,
            provider: provider.into(),
            refresh_token: None,
            id_token: None,
            account_id: Some("acct_1".into()),
            plan_type: None,
            credits_balance: None,
            credits_unlimited: 0,
            codex_warmup_at: warmup_at,
        }
    }

    fn snapshot(five_hour: f64, seven_day: f64, status: RateLimitStatus) -> UsageSnapshot {
        let window = |utilization: f64| WindowState {
            utilization,
            reset_at: 1_000_600,
            status: RateLimitStatus::Allowed,
        };
        UsageSnapshot {
            five_hour: window(five_hour),
            seven_day: window(seven_day),
            seven_day_fable: None,
            unified_status: status,
            representative_claim: None,
            probed_at: 1_000_000,
        }
    }

    const NOW: i64 = 1_000_000;

    #[test]
    fn idle_codex_credential_is_due() {
        assert!(codex_warmup_due(
            &row("codex", None),
            &snapshot(0.0, 0.0, RateLimitStatus::Allowed),
            NOW
        ));
    }

    #[test]
    fn any_usage_means_clock_already_running() {
        assert!(!codex_warmup_due(
            &row("codex", None),
            &snapshot(0.01, 0.0, RateLimitStatus::Allowed),
            NOW
        ));
        assert!(!codex_warmup_due(
            &row("codex", None),
            &snapshot(0.0, 0.2, RateLimitStatus::Allowed),
            NOW
        ));
    }

    #[test]
    fn recent_warmup_suppresses_until_window_length_passes() {
        let idle = snapshot(0.0, 0.0, RateLimitStatus::Allowed);
        assert!(!codex_warmup_due(
            &row("codex", Some(NOW - CODEX_WARMUP_MIN_INTERVAL_SECS + 1)),
            &idle,
            NOW
        ));
        assert!(codex_warmup_due(
            &row("codex", Some(NOW - CODEX_WARMUP_MIN_INTERVAL_SECS)),
            &idle,
            NOW
        ));
    }

    #[test]
    fn claude_disabled_and_rejected_rows_are_never_due() {
        let idle = snapshot(0.0, 0.0, RateLimitStatus::Allowed);
        assert!(!codex_warmup_due(&row("claude", None), &idle, NOW));
        let mut disabled = row("codex", None);
        disabled.disabled_at = Some(NOW);
        assert!(!codex_warmup_due(&disabled, &idle, NOW));
        assert!(!codex_warmup_due(
            &row("codex", None),
            &snapshot(0.0, 0.0, RateLimitStatus::Rejected),
            NOW
        ));
    }
}
