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

use serde::{Deserialize, Deserializer};

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
    secondary_window: Option<WindowBlock>,
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

#[derive(Debug, Clone)]
struct CreditsBlock {
    unlimited: Option<bool>,
    balance: Option<String>,
}

impl<'de> Deserialize<'de> for CreditsBlock {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct RawCreditsBlock {
            unlimited: Option<bool>,
            balance: Option<serde_json::Value>,
        }
        let raw = RawCreditsBlock::deserialize(deserializer)?;
        let balance = match raw.balance {
            None => None,
            Some(serde_json::Value::String(s)) => Some(s),
            Some(serde_json::Value::Number(n)) => Some(n.to_string()),
            Some(other) => {
                return Err(serde::de::Error::custom(format!(
                    "credits.balance must be a string or number, got {other}"
                )));
            }
        };
        Ok(CreditsBlock {
            unlimited: raw.unlimited,
            balance,
        })
    }
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
    let secondary_block = body
        .rate_limit
        .secondary_window
        .as_ref()
        .unwrap_or(&body.rate_limit.primary_window);
    let secondary = window_state(secondary_block, limit_reached, probed_at)?;
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
        let primary_util = primary.utilization;
        let secondary_util = secondary.utilization;
        if limit_reached && secondary_util > primary_util {
            Some("seven_day".to_string())
        } else if matches!(primary.status, RateLimitStatus::Rejected) {
            Some("five_hour".to_string())
        } else {
            Some("seven_day".to_string())
        }
    } else {
        None
    };

    let snapshot = UsageSnapshot {
        five_hour: primary,
        seven_day_fable: None,
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
    let reset_at = block.reset_at.or_else(|| {
        block
            .reset_after_seconds
            .map(|secs| probed_at.saturating_add(secs))
    });
    // Some WHAM responses omit reset timestamps when a window is already full.
    // Fall back to a short park window so utilization drives status without
    // pinning cooldowns centuries ahead (matches compute_cooldown_until fallback).
    let reset_at = reset_at.unwrap_or_else(|| probed_at.saturating_add(600));
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
