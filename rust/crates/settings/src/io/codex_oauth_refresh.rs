//! Codex OAuth refresh-token exchange.
//!
//! See PR #1006. Refresh before handing credentials to Codex when the
//! access-token JWT is near expiry or when the stored token bundle is stale.

use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, OnceLock};

use serde::Deserialize;
use tokio::sync::Mutex;

/// OpenAI's OAuth client id, hard-coded by the Codex CLI.
pub const CODEX_OAUTH_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
/// OAuth token endpoint.
pub const REFRESH_TOKEN_URL: &str = "https://auth.openai.com/oauth/token";

/// Stale-token threshold: refresh if the stored token bundle is older than 7 days.
pub const REFRESH_STALE_TOKEN_SECS: i64 = 7 * 24 * 60 * 60;
/// Expiry threshold: refresh if the access-token JWT expires within 5 minutes.
pub const REFRESH_EXPIRY_BUFFER_SECS: i64 = 5 * 60;

static REFRESH_LOCKS: OnceLock<std::sync::Mutex<HashMap<i64, Arc<Mutex<()>>>>> = OnceLock::new();

/// Serialize refresh-token exchanges for a single credential row. OpenAI
/// rotates refresh tokens, so two concurrent refreshes with the same stored
/// token can otherwise make the losing request look like a permanent token
/// reuse and kill a live credential.
pub async fn with_credential_refresh_lock<T, F, Fut>(credential_id: i64, work: F) -> T
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = T>,
{
    let lock = {
        let locks = REFRESH_LOCKS.get_or_init(|| std::sync::Mutex::new(HashMap::new()));
        let mut guards = locks
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        guards
            .entry(credential_id)
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    };
    let _guard = lock.lock().await;
    work().await
}

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

#[derive(Debug, Deserialize)]
struct RefreshResponse {
    access_token: String,
    refresh_token: Option<String>,
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
    let response = client
        .post(REFRESH_TOKEN_URL)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", CODEX_OAUTH_CLIENT_ID),
            ("scope", "openid profile email"),
        ])
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
        refresh_token: parsed
            .refresh_token
            .unwrap_or_else(|| refresh_token.to_string()),
        id_token: parsed.id_token,
    })
}

/// Decide whether a Codex credential needs a proactive refresh before the
/// next probe / pick.
pub fn should_refresh(
    access_token_exp_secs: Option<i64>,
    token_updated_at_secs: Option<i64>,
    now_secs: i64,
) -> bool {
    if access_token_exp_secs.is_some_and(|exp| exp - now_secs <= REFRESH_EXPIRY_BUFFER_SECS) {
        return true;
    }
    token_updated_at_secs.is_some_and(|last| now_secs - last >= REFRESH_STALE_TOKEN_SECS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_refresh_when_access_token_expired_or_close() {
        let now = 1_000_000;
        assert!(should_refresh(Some(now - 1), None, now));
        assert!(should_refresh(
            Some(now + REFRESH_EXPIRY_BUFFER_SECS),
            None,
            now
        ));
        assert!(!should_refresh(
            Some(now + REFRESH_EXPIRY_BUFFER_SECS + 1),
            None,
            now
        ));
    }

    #[test]
    fn should_refresh_when_token_bundle_older_than_7d() {
        let now = 10_000_000;
        let week = 7 * 24 * 3600;
        assert!(should_refresh(None, Some(now - week - 1), now));
        assert!(!should_refresh(None, Some(now - week + 1), now));
    }

    #[test]
    fn should_not_refresh_when_no_data() {
        assert!(!should_refresh(None, None, 1_000_000));
    }

    #[test]
    fn refresh_response_allows_omitted_refresh_token() {
        let parsed: RefreshResponse =
            serde_json::from_str(r#"{"access_token":"new-access"}"#).expect("parse response");
        assert_eq!(parsed.access_token, "new-access");
        assert_eq!(parsed.refresh_token, None);
    }
}
