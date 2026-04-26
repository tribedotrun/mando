//! Codex OAuth refresh-token exchange.
//!
//! See PR #1006. Mirrors `codex-rs/login/src/auth/manager.rs:1735-1743` —
//! we proactively refresh when the JWT exp is within 5 minutes of now or
//! when the credential's `last_probed_at` is older than 7 days. The probe
//! path also falls back to a reactive 401-retry refresh as a safety net.

use serde::{Deserialize, Serialize};

/// OpenAI's OAuth client id, hard-coded by the Codex CLI.
pub const CODEX_OAUTH_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
/// OAuth token endpoint.
pub const REFRESH_TOKEN_URL: &str = "https://auth.openai.com/oauth/token";

/// JWT-exp threshold below which we proactively refresh: 5 minutes.
pub const REFRESH_EXP_THRESHOLD_SECS: i64 = 300;
/// Stale-probe threshold: refresh if `last_probed_at` is older than 7 days.
pub const REFRESH_STALE_PROBE_SECS: i64 = 7 * 24 * 60 * 60;

#[derive(Debug, Clone)]
pub struct RefreshedTokens {
    pub access_token: String,
    pub refresh_token: String,
    /// Optional: OpenAI sometimes returns a fresh id_token, sometimes not.
    pub id_token: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum RefreshError {
    /// `refresh_token_expired` / `_reused` / `_invalidated` from upstream.
    /// The credential is dead; mark it expired and stop trying.
    #[error("refresh token permanently invalid: {0}")]
    Permanent(String),
    /// 401 from the token endpoint without a permanent error code. Should
    /// not happen in practice but treated as transient just in case.
    #[error("unauthorized")]
    Unauthorized,
    /// Upstream returned 4xx/5xx that isn't a permanent invalidation.
    #[error("transient HTTP {status}: {body}")]
    Http { status: u16, body: String },
    /// Network-level failure (timeout, DNS, etc.).
    #[error("network error: {0}")]
    Network(String),
    /// Response shape didn't match expectations.
    #[error("parse error: {0}")]
    Parse(String),
}

#[derive(Debug, Serialize)]
struct RefreshRequest<'a> {
    client_id: &'a str,
    grant_type: &'a str,
    refresh_token: &'a str,
}

#[derive(Debug, Deserialize)]
struct RefreshResponse {
    access_token: String,
    refresh_token: String,
    id_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RefreshErrorBody {
    error: Option<String>,
}

/// POST `auth.openai.com/oauth/token` to swap a refresh_token for a fresh
/// access_token + refresh_token (and possibly id_token).
pub async fn refresh(refresh_token: &str) -> Result<RefreshedTokens, RefreshError> {
    let client = global_net::http::codex_probe_client();
    let body = RefreshRequest {
        client_id: CODEX_OAUTH_CLIENT_ID,
        grant_type: "refresh_token",
        refresh_token,
    };
    let response = client
        .post(REFRESH_TOKEN_URL)
        .header("Content-Type", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| RefreshError::Network(e.to_string()))?;

    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return Err(RefreshError::Unauthorized);
    }
    if !status.is_success() {
        let body_text = response.text().await.unwrap_or_default();
        if let Ok(parsed) = serde_json::from_str::<RefreshErrorBody>(&body_text) {
            if let Some(code) = parsed.error.as_deref() {
                if matches!(
                    code,
                    "refresh_token_expired" | "refresh_token_reused" | "refresh_token_invalidated"
                ) {
                    return Err(RefreshError::Permanent(code.to_string()));
                }
            }
        }
        return Err(RefreshError::Http {
            status: status.as_u16(),
            body: body_text.chars().take(500).collect(),
        });
    }

    let parsed: RefreshResponse = response
        .json()
        .await
        .map_err(|e| RefreshError::Parse(e.to_string()))?;
    Ok(RefreshedTokens {
        access_token: parsed.access_token,
        refresh_token: parsed.refresh_token,
        id_token: parsed.id_token,
    })
}

/// Decide whether a Codex credential needs a proactive refresh before the
/// next probe / activate write. Mirrors Codex CLI's own check.
///
/// `id_token_exp_secs` is the JWT exp claim (`exp`); `last_probed_at_secs`
/// is the last probe timestamp from the row. Either may be `None` for a
/// freshly added credential — in which case we skip the proactive refresh
/// (the very next probe will see `last_probed_at` advance).
pub fn should_refresh(
    id_token_exp_secs: Option<i64>,
    last_probed_at_secs: Option<i64>,
    now_secs: i64,
) -> bool {
    if let Some(exp) = id_token_exp_secs {
        if exp - now_secs <= REFRESH_EXP_THRESHOLD_SECS {
            return true;
        }
    }
    if let Some(last) = last_probed_at_secs {
        if now_secs - last >= REFRESH_STALE_PROBE_SECS {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_refresh_when_exp_within_5min() {
        assert!(should_refresh(Some(1_000_000), None, 1_000_000 - 100));
        assert!(should_refresh(Some(1_000_000), None, 999_700));
    }

    #[test]
    fn should_not_refresh_when_exp_far_future() {
        assert!(!should_refresh(Some(1_000_000), None, 990_000));
    }

    #[test]
    fn should_refresh_when_last_probe_older_than_7d() {
        let now = 10_000_000;
        let week = 7 * 24 * 3600;
        assert!(should_refresh(None, Some(now - week - 1), now));
        assert!(!should_refresh(None, Some(now - week + 1), now));
    }

    #[test]
    fn should_not_refresh_when_no_data() {
        assert!(!should_refresh(None, None, 1_000_000));
    }
}
