//! Credential wire types shared between Claude and Codex providers.
//!
//! Codex-specific add/active/activate request and response types live in
//! `credentials_codex.rs`. The shape rendered to the UI for both providers
//! is `CredentialInfo` here, with Codex-only fields hanging off the optional
//! `codex` member.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::credentials_codex::{CodexCredentialDetails, CredentialProvider};

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CredentialInfo {
    pub id: i64,
    pub label: String,
    pub token_masked: String,
    pub provider: CredentialProvider,
    pub expires_at: Option<i64>,
    pub rate_limit_cooldown_until: Option<i64>,
    pub created_at: String,
    pub is_expired: bool,
    pub is_rate_limited: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub five_hour: Option<CredentialWindowInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub seven_day: Option<CredentialWindowInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub unified_status: Option<CredentialRateLimitStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub representative_claim: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub last_probed_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub cost_since_probe_usd: Option<f64>,
    /// Set only when `provider == Codex`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional = nullable)]
    pub codex: Option<CodexCredentialDetails>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum CredentialRateLimitStatus {
    Allowed,
    AllowedWarning,
    Rejected,
}

pub type RateLimitStatus = CredentialRateLimitStatus;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CredentialWindowInfo {
    pub utilization: f64,
    pub reset_at: i64,
    pub status: CredentialRateLimitStatus,
}

pub type UsageWindowState = CredentialWindowInfo;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CredentialUsageSnapshot {
    pub five_hour: CredentialWindowInfo,
    pub seven_day: CredentialWindowInfo,
    pub unified_status: CredentialRateLimitStatus,
    pub representative_claim: Option<String>,
    pub probed_at: i64,
}
