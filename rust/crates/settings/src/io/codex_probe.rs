//! Codex credential usage probe.
//!
//! See PR #1006. Hits `chatgpt.com/backend-api/wham/usage` with a Bearer
//! access_token + `chatgpt-account-id` header, parses the response body,
//! and returns the same `UsageSnapshot` shape Claude's probe produces so
//! `pick_for_worker` keeps one query.
//!
//! The endpoint was confirmed by live probe against two real auth.json
//! files (`b_aburra` + `b_gmail` accounts on this developer's machine).
//! Codex CLI's source ships a test fixture at `/api/codex/usage` but that
//! path returns 403 from production Cloudflare; only `/backend-api/wham/usage`
//! actually responds.

use serde::Deserialize;

use crate::io::usage_probe::{ProbeError, UsageSnapshot, WindowState};
use global_types::RateLimitStatus;

const USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";

/// Codex-side probe response. We intentionally only capture the fields we
/// render or persist; OpenAI may add unrelated keys without breaking us.
#[derive(Debug, Clone, Deserialize)]
struct UsageResponse {
    plan_type: Option<String>,
    rate_limit: RateLimitBlock,
    credits: Option<CreditsBlock>,
}

#[derive(Debug, Clone, Deserialize)]
struct RateLimitBlock {
    primary_window: WindowBlock,
    secondary_window: WindowBlock,
    /// Optional flag — when `Some(true)`, both windows are unusable.
    limit_reached: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
struct WindowBlock {
    used_percent: f64,
    /// Unix seconds when the window resets.
    reset_at: Option<i64>,
    /// Seconds from now until reset (alternative to `reset_at`).
    reset_after_seconds: Option<i64>,
}

#[derive(Debug, Clone, Deserialize)]
struct CreditsBlock {
    unlimited: Option<bool>,
    balance: Option<String>,
}

/// Captured details from a successful probe. The `UsageSnapshot` is what
/// the existing pick_for_worker / cooldown machinery cares about; the
/// extras (`plan_type`, credits) are persisted on the credential row.
#[derive(Debug, Clone)]
pub struct CodexProbeOutcome {
    pub snapshot: UsageSnapshot,
    pub plan_type: Option<String>,
    pub credits_balance: Option<String>,
    pub credits_unlimited: bool,
}

/// GET `chatgpt.com/backend-api/wham/usage` and parse the response into a
/// `UsageSnapshot`. The caller is responsible for refresh-on-401 retry —
/// this function returns `ProbeError::Unauthorized` and lets the dispatcher
/// decide whether to refresh and call again.
pub async fn probe(
    access_token: &str,
    account_id: Option<&str>,
) -> Result<CodexProbeOutcome, ProbeError> {
    let client = global_net::http::codex_probe_client();
    let mut request = client.get(USAGE_URL).bearer_auth(access_token);
    if let Some(acct) = account_id {
        request = request.header("chatgpt-account-id", acct);
    }
    let response = request
        .send()
        .await
        .map_err(|e| ProbeError::Network(e.to_string()))?;

    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(ProbeError::Unauthorized);
    }
    if !status.is_success() {
        return Err(ProbeError::Http(status.as_u16()));
    }
    let body: UsageResponse = response
        .json()
        .await
        .map_err(|e| ProbeError::Parse(e.to_string()))?;
    parse_outcome(body)
}

fn parse_outcome(body: UsageResponse) -> Result<CodexProbeOutcome, ProbeError> {
    let probed_at = time::OffsetDateTime::now_utc().unix_timestamp();
    let limit_reached = body.rate_limit.limit_reached.unwrap_or(false);
    let primary = window_state(&body.rate_limit.primary_window, limit_reached, probed_at)?;
    let secondary = window_state(&body.rate_limit.secondary_window, limit_reached, probed_at)?;
    let unified_status = match (&primary.status, &secondary.status) {
        (RateLimitStatus::Rejected, _) | (_, RateLimitStatus::Rejected) => {
            RateLimitStatus::Rejected
        }
        (RateLimitStatus::AllowedWarning, _) | (_, RateLimitStatus::AllowedWarning) => {
            RateLimitStatus::AllowedWarning
        }
        _ => RateLimitStatus::Allowed,
    };

    let representative_claim = if matches!(unified_status, RateLimitStatus::Rejected) {
        if matches!(primary.status, RateLimitStatus::Rejected) {
            Some("five_hour".to_string())
        } else {
            Some("seven_day".to_string())
        }
    } else {
        None
    };

    let snapshot = UsageSnapshot {
        five_hour: primary,
        seven_day: secondary,
        unified_status,
        representative_claim,
        probed_at,
    };

    let credits = body.credits.unwrap_or(CreditsBlock {
        unlimited: None,
        balance: None,
    });

    Ok(CodexProbeOutcome {
        snapshot,
        plan_type: body.plan_type,
        credits_balance: credits.balance,
        credits_unlimited: credits.unlimited.unwrap_or(false),
    })
}

fn window_state(
    block: &WindowBlock,
    global_limit_reached: bool,
    probed_at: i64,
) -> Result<WindowState, ProbeError> {
    let utilization = (block.used_percent / 100.0).clamp(0.0, 1.0);
    let reset_at = block
        .reset_at
        .or_else(|| {
            block
                .reset_after_seconds
                .map(|secs| probed_at.saturating_add(secs))
        })
        .ok_or_else(|| {
            ProbeError::Parse("window has neither reset_at nor reset_after_seconds".to_string())
        })?;
    let status = if utilization >= 1.0 || global_limit_reached {
        RateLimitStatus::Rejected
    } else if utilization >= 0.8 {
        RateLimitStatus::AllowedWarning
    } else {
        RateLimitStatus::Allowed
    };
    Ok(WindowState {
        utilization,
        reset_at,
        status,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body_with(primary_pct: f64, secondary_pct: f64, limit_reached: bool) -> UsageResponse {
        UsageResponse {
            plan_type: Some("pro".to_string()),
            rate_limit: RateLimitBlock {
                primary_window: WindowBlock {
                    used_percent: primary_pct,
                    reset_at: Some(1_777_173_104),
                    reset_after_seconds: Some(10618),
                },
                secondary_window: WindowBlock {
                    used_percent: secondary_pct,
                    reset_at: Some(1_777_414_856),
                    reset_after_seconds: Some(252_370),
                },
                limit_reached: Some(limit_reached),
            },
            credits: Some(CreditsBlock {
                unlimited: Some(false),
                balance: Some("0".to_string()),
            }),
        }
    }

    #[test]
    fn parse_happy_path_pro_plan() {
        let outcome = parse_outcome(body_with(4.0, 22.0, false)).unwrap();
        assert_eq!(outcome.plan_type.as_deref(), Some("pro"));
        assert!((outcome.snapshot.five_hour.utilization - 0.04).abs() < 1e-9);
        assert!((outcome.snapshot.seven_day.utilization - 0.22).abs() < 1e-9);
        assert_eq!(outcome.snapshot.unified_status, RateLimitStatus::Allowed);
        assert_eq!(outcome.credits_balance.as_deref(), Some("0"));
    }

    #[test]
    fn parse_warning_threshold() {
        let outcome = parse_outcome(body_with(82.0, 22.0, false)).unwrap();
        assert_eq!(
            outcome.snapshot.five_hour.status,
            RateLimitStatus::AllowedWarning
        );
        assert_eq!(
            outcome.snapshot.unified_status,
            RateLimitStatus::AllowedWarning
        );
    }

    #[test]
    fn parse_rejected_when_limit_reached_flag() {
        let outcome = parse_outcome(body_with(99.5, 50.0, true)).unwrap();
        assert_eq!(outcome.snapshot.unified_status, RateLimitStatus::Rejected);
        assert_eq!(
            outcome.snapshot.representative_claim.as_deref(),
            Some("five_hour")
        );
    }

    #[test]
    fn parse_rejected_when_secondary_full() {
        let outcome = parse_outcome(body_with(50.0, 100.0, false)).unwrap();
        assert_eq!(outcome.snapshot.unified_status, RateLimitStatus::Rejected);
        assert_eq!(
            outcome.snapshot.representative_claim.as_deref(),
            Some("seven_day")
        );
    }
}
