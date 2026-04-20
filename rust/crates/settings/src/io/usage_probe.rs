//! Proactive credential usage probe.
//!
//! Sends a 1-token Haiku request to `/v1/messages` with the OAuth access
//! token and reads the `anthropic-ratelimit-unified-*` response headers.
//! Both 200 and 429 responses carry the same headers, so either tells us
//! the current utilization for the five-hour and seven-day windows.
//!
//! Cost per probe: ~1 Haiku output token (~$0.00004). Counts against the
//! quota but negligibly.
//!
//! Approach mirrors the pattern used by claude-monitor (github.com/rjwalters
//! /claude-monitor), which abandoned the undocumented `/api/oauth/usage`
//! endpoint after it started rejecting OAuth tokens; the ping is the only
//! working way to read live utilization today.
//!
//! Verified working end-to-end against real OAuth tokens; see the plan doc
//! for the reference `curl` invocation.
use serde::Serialize;

/// Model used for the 1-token ping. Dated ID pins us against alias drift.
const PROBE_MODEL: &str = "claude-haiku-4-5-20251001";

/// Anthropic rate-limit status as reported on `anthropic-ratelimit-unified-*-status`.
/// Canonical type lives in `global-types::rate_limit`; this re-export keeps
/// the public surface of `settings` stable.
pub use global_types::RateLimitStatus;

/// State of a single rate-limit window.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WindowState {
    /// Fraction of the window consumed, in `[0.0, 1.0]`.
    pub utilization: f64,
    /// Unix seconds when the window resets.
    pub reset_at: i64,
    pub status: RateLimitStatus,
}

/// Snapshot of one credential's usage across both windows.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSnapshot {
    pub five_hour: WindowState,
    pub seven_day: WindowState,
    pub unified_status: RateLimitStatus,
    /// Which window the server treats as binding right now
    /// (e.g. `five_hour`, `seven_day`, `seven_day_opus`).
    pub representative_claim: Option<String>,
    /// Unix seconds when this snapshot was captured.
    pub probed_at: i64,
}

/// Probe failure modes.
#[derive(Debug, thiserror::Error)]
pub enum ProbeError {
    /// Token rejected as expired or invalid. Caller should mark credential
    /// expired and stop probing until the user re-authenticates.
    #[error("unauthorized (token expired or invalid)")]
    Unauthorized,
    /// Unexpected HTTP status other than 200, 401, or 429.
    #[error("unexpected HTTP status {0}")]
    Http(u16),
    /// Network-level failure (connect/timeout/DNS).
    #[error("network error: {0}")]
    Network(String),
    /// Response was 200/429 but required rate-limit headers were missing.
    /// Indicates the API response shape has changed or the account tier
    /// does not return unified headers — not a transient blip.
    #[error("parse error: {0}")]
    Parse(String),
    /// The probe succeeded but persisting the snapshot to the DB failed.
    /// Callers that care about `last_probed_at` advancing (the poll
    /// throttle, the pre-spawn staleness check) must treat this as a
    /// hard failure, not a transient one.
    #[error("persist error: {0}")]
    Persist(String),
}

/// POST a 1-token ping to the Anthropic API with `access_token` and return a
/// parsed snapshot.
///
/// Both 200 and 429 carry the rate-limit headers; 401 means the OAuth token
/// is dead. 5xx and network errors are transient; caller should retry on the
/// next poll tick.
pub async fn probe(access_token: &str) -> Result<UsageSnapshot, ProbeError> {
    let client = global_net::http::usage_probe_client();

    let body = serde_json::json!({
        "model": PROBE_MODEL,
        "max_tokens": 1,
        "messages": [{"role": "user", "content": "x"}],
    });

    let response = client
        .post("https://api.anthropic.com/v1/messages")
        .bearer_auth(access_token)
        .header("anthropic-version", "2023-06-01")
        .header("anthropic-beta", "oauth-2025-04-20")
        .json(&body)
        .send()
        .await
        .map_err(|e| ProbeError::Network(e.to_string()))?;

    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(ProbeError::Unauthorized);
    }
    if status != reqwest::StatusCode::OK && status != reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Err(ProbeError::Http(status.as_u16()));
    }

    parse_headers(response.headers())
}

fn parse_headers(headers: &reqwest::header::HeaderMap) -> Result<UsageSnapshot, ProbeError> {
    let probed_at = time::OffsetDateTime::now_utc().unix_timestamp();
    let five_hour = parse_window(headers, "5h")?;
    let seven_day = parse_window(headers, "7d")?;
    let unified_status = required_status(headers, "anthropic-ratelimit-unified-status")?;
    let representative_claim = headers
        .get("anthropic-ratelimit-unified-representative-claim")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    Ok(UsageSnapshot {
        five_hour,
        seven_day,
        unified_status,
        representative_claim,
        probed_at,
    })
}

fn parse_window(
    headers: &reqwest::header::HeaderMap,
    suffix: &str,
) -> Result<WindowState, ProbeError> {
    let util_hdr = format!("anthropic-ratelimit-unified-{suffix}-utilization");
    let reset_hdr = format!("anthropic-ratelimit-unified-{suffix}-reset");
    let status_hdr = format!("anthropic-ratelimit-unified-{suffix}-status");

    let utilization = required_f64(headers, &util_hdr)?;
    let reset_at = required_i64(headers, &reset_hdr)?;
    let status = required_status(headers, &status_hdr)?;
    Ok(WindowState {
        utilization,
        reset_at,
        status,
    })
}

fn required_f64(headers: &reqwest::header::HeaderMap, name: &str) -> Result<f64, ProbeError> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<f64>().ok())
        .ok_or_else(|| ProbeError::Parse(format!("missing or invalid header: {name}")))
}

fn required_i64(headers: &reqwest::header::HeaderMap, name: &str) -> Result<i64, ProbeError> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<i64>().ok())
        .ok_or_else(|| ProbeError::Parse(format!("missing or invalid header: {name}")))
}

fn required_status(
    headers: &reqwest::header::HeaderMap,
    name: &str,
) -> Result<RateLimitStatus, ProbeError> {
    let s = headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| ProbeError::Parse(format!("missing or invalid header: {name}")))?;
    let status = RateLimitStatus::parse(s);
    if status.is_known() {
        Ok(status)
    } else {
        Err(ProbeError::Parse(format!(
            "unexpected rate-limit status {s:?} for header {name}"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reqwest::header::{HeaderMap, HeaderValue};

    fn canned_headers() -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(
            "anthropic-ratelimit-unified-5h-utilization",
            HeaderValue::from_static("0.47"),
        );
        h.insert(
            "anthropic-ratelimit-unified-5h-reset",
            HeaderValue::from_static("1776420000"),
        );
        h.insert(
            "anthropic-ratelimit-unified-5h-status",
            HeaderValue::from_static("allowed_warning"),
        );
        h.insert(
            "anthropic-ratelimit-unified-7d-utilization",
            HeaderValue::from_static("0.08"),
        );
        h.insert(
            "anthropic-ratelimit-unified-7d-reset",
            HeaderValue::from_static("1776970800"),
        );
        h.insert(
            "anthropic-ratelimit-unified-7d-status",
            HeaderValue::from_static("allowed"),
        );
        h.insert(
            "anthropic-ratelimit-unified-status",
            HeaderValue::from_static("allowed_warning"),
        );
        h.insert(
            "anthropic-ratelimit-unified-representative-claim",
            HeaderValue::from_static("five_hour"),
        );
        h
    }

    #[test]
    fn parse_headers_happy_path() {
        let snap = parse_headers(&canned_headers()).expect("parse");
        assert!((snap.five_hour.utilization - 0.47).abs() < 1e-9);
        assert_eq!(snap.five_hour.reset_at, 1_776_420_000);
        assert_eq!(snap.five_hour.status, RateLimitStatus::AllowedWarning);
        assert!((snap.seven_day.utilization - 0.08).abs() < 1e-9);
        assert_eq!(snap.seven_day.status, RateLimitStatus::Allowed);
        assert_eq!(snap.unified_status, RateLimitStatus::AllowedWarning);
        assert_eq!(snap.representative_claim.as_deref(), Some("five_hour"));
    }

    #[test]
    fn parse_headers_missing_field_errs() {
        let mut h = canned_headers();
        h.remove("anthropic-ratelimit-unified-5h-reset");
        let err = parse_headers(&h).unwrap_err();
        assert!(matches!(err, ProbeError::Parse(_)));
    }

    #[test]
    fn ratelimit_status_roundtrip() {
        for s in [
            RateLimitStatus::Allowed,
            RateLimitStatus::AllowedWarning,
            RateLimitStatus::Rejected,
        ] {
            assert_eq!(RateLimitStatus::parse(s.as_str()), s);
        }
        assert!(!RateLimitStatus::parse("bogus").is_known());
    }
}
