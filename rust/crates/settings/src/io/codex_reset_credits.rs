//! Codex rate-limit reset credit probe.
//!
//! Hits `chatgpt.com/backend-api/wham/rate-limit-reset-credits` with the
//! stored ChatGPT OAuth access token + account id. This endpoint reports
//! available free reset credits separately from the 5h/7d usage endpoint.

use serde::Deserialize;

use crate::io::usage_probe::ProbeError;

const RESET_CREDITS_URL: &str = "https://chatgpt.com/backend-api/wham/rate-limit-reset-credits";

#[derive(Debug, Clone, Deserialize)]
struct ResetCreditsResponse {
    available_count: i64,
    total_earned_count: i64,
    credits: Vec<ResetCreditBlock>,
}

#[derive(Debug, Clone, Deserialize)]
struct ResetCreditBlock {
    title: String,
    description: Option<String>,
    expires_at: String,
    granted_at: Option<String>,
    status: Option<String>,
    redeem_started_at: Option<String>,
    redeemed_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct CodexResetCreditsOutcome {
    pub available_count: i64,
    pub total_earned_count: i64,
    pub credits: Vec<CodexResetCreditOutcome>,
}

#[derive(Debug, Clone)]
pub struct CodexResetCreditOutcome {
    pub title: String,
    pub description: Option<String>,
    pub expires_at: i64,
    pub granted_at: Option<i64>,
}

/// GET `chatgpt.com/backend-api/wham/rate-limit-reset-credits` and parse
/// available Codex reset credits. The caller owns refresh-on-401 retry.
pub async fn fetch(
    access_token: &str,
    account_id: &str,
) -> Result<CodexResetCreditsOutcome, ProbeError> {
    let response = global_net::http::codex_probe_client()
        .get(RESET_CREDITS_URL)
        .bearer_auth(access_token)
        .header("ChatGPT-Account-ID", account_id)
        .header(reqwest::header::ACCEPT, "application/json")
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

    let body: ResetCreditsResponse = response
        .json()
        .await
        .map_err(|e| ProbeError::Parse(e.to_string()))?;
    parse_outcome(body)
}

fn parse_outcome(body: ResetCreditsResponse) -> Result<CodexResetCreditsOutcome, ProbeError> {
    let credits = body
        .credits
        .into_iter()
        .filter(is_available_credit)
        .map(parse_credit)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(CodexResetCreditsOutcome {
        available_count: body.available_count,
        total_earned_count: body.total_earned_count,
        credits,
    })
}

fn is_available_credit(credit: &ResetCreditBlock) -> bool {
    credit.status.as_deref() == Some("available")
        && credit.redeem_started_at.is_none()
        && credit.redeemed_at.is_none()
}

fn parse_credit(credit: ResetCreditBlock) -> Result<CodexResetCreditOutcome, ProbeError> {
    Ok(CodexResetCreditOutcome {
        title: credit.title,
        description: credit.description,
        expires_at: parse_epoch_secs(&credit.expires_at)?,
        granted_at: match credit.granted_at.as_deref() {
            Some(value) => Some(parse_epoch_secs(value)?),
            None => None,
        },
    })
}

fn parse_epoch_secs(value: &str) -> Result<i64, ProbeError> {
    time::OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339)
        .map(|dt| dt.unix_timestamp())
        .map_err(|e| ProbeError::Parse(e.to_string()))
}
