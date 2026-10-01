//! Last successful Desktop billing data plus the most recent refresh outcome.
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClaudeSubscriptionInfo {
    pub plan_name: Option<String>,
    pub billing_interval: Option<String>,
    /// Provider-owned subscription status (separate from credential status).
    pub status: Option<String>,
    /// Unix milliseconds. A scheduled end takes precedence over renewal.
    pub renews_at: Option<i64>,
    pub ends_at: Option<i64>,
    /// Provider calendar dates, used only when precise timestamps are absent.
    pub ends_before: Option<String>,
    pub next_charge_date: Option<String>,
    /// Unix milliseconds of the last complete successful refresh.
    pub checked_at: Option<i64>,
    /// Unix milliseconds of the latest attempt, including failed attempts.
    pub attempted_at: Option<i64>,
    pub error: Option<String>,
    pub keychain_access_required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClaudeDesktopKeychainAuthorization {
    pub authorized: bool,
}
