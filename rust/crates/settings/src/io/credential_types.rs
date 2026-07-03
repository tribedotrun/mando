//! Public types for credentials — `CredentialRow`, the public
//! `CredentialInfo` payload, and helpers that map between them.

/// A credential row from the database.
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct CredentialRow {
    pub id: i64,
    pub label: String,
    pub access_token: String,
    pub expires_at: Option<i64>,
    pub rate_limit_cooldown_until: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
    pub five_hour_utilization: Option<f64>,
    pub five_hour_reset_at: Option<i64>,
    pub five_hour_status: Option<String>,
    pub seven_day_utilization: Option<f64>,
    pub seven_day_reset_at: Option<i64>,
    pub seven_day_status: Option<String>,
    pub unified_status: Option<String>,
    pub representative_claim: Option<String>,
    pub last_probed_at: Option<i64>,
    pub last_picked_at: Option<i64>,
    pub token_updated_at: Option<i64>,
    pub provider: String,
    pub refresh_token: Option<String>,
    pub id_token: Option<String>,
    pub account_id: Option<String>,
    pub plan_type: Option<String>,
    pub credits_balance: Option<String>,
    pub credits_unlimited: i64,
}

/// Per-window usage snapshot included in the public credential info payload.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialWindowInfo {
    pub utilization: f64,
    pub reset_at: i64,
    pub status: String,
}

/// Codex-only fields surfaced on `CredentialInfo` for `provider == "codex"` rows.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexInfo {
    pub account_id: String,
    pub plan_type: Option<String>,
    pub credits_balance: Option<String>,
    pub credits_unlimited: bool,
}

/// Public credential info (no secrets).
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialInfo {
    pub id: i64,
    pub label: String,
    pub token_masked: String,
    pub provider: String,
    pub expires_at: Option<i64>,
    pub rate_limit_cooldown_until: Option<i64>,
    pub created_at: String,
    pub is_expired: bool,
    pub is_rate_limited: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub five_hour: Option<CredentialWindowInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seven_day: Option<CredentialWindowInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unified_status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub representative_claim: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_probed_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_since_probe_usd: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub codex: Option<CodexInfo>,
}

impl CredentialRow {
    pub fn to_info(&self) -> CredentialInfo {
        let now_ms = time::OffsetDateTime::now_utc().unix_timestamp() * 1000;
        let now_secs = now_ms / 1000;
        let codex = if self.provider == "codex" {
            self.account_id.clone().map(|account_id| CodexInfo {
                account_id,
                plan_type: self.plan_type.clone(),
                credits_balance: self.credits_balance.clone(),
                credits_unlimited: self.credits_unlimited != 0,
            })
        } else {
            None
        };
        CredentialInfo {
            id: self.id,
            label: self.label.clone(),
            token_masked: mask_token(&self.access_token),
            provider: self.provider.clone(),
            expires_at: self.expires_at,
            rate_limit_cooldown_until: self.rate_limit_cooldown_until,
            created_at: self.created_at.clone(),
            is_expired: self.expires_at.is_some_and(|ea| ea <= now_ms),
            is_rate_limited: self
                .rate_limit_cooldown_until
                .is_some_and(|until| now_secs < until),
            five_hour: window_info(
                self.five_hour_utilization,
                self.five_hour_reset_at,
                self.five_hour_status.as_deref(),
            ),
            seven_day: window_info(
                self.seven_day_utilization,
                self.seven_day_reset_at,
                self.seven_day_status.as_deref(),
            ),
            unified_status: self.unified_status.clone(),
            representative_claim: self.representative_claim.clone(),
            last_probed_at: self.last_probed_at,
            cost_since_probe_usd: None,
            codex,
        }
    }
}

fn window_info(
    util: Option<f64>,
    reset: Option<i64>,
    status: Option<&str>,
) -> Option<CredentialWindowInfo> {
    match (util, reset, status) {
        (Some(u), Some(r), Some(s)) => Some(CredentialWindowInfo {
            utilization: u,
            reset_at: r,
            status: s.to_string(),
        }),
        _ => None,
    }
}

fn mask_token(token: &str) -> String {
    let char_count = token.chars().count();
    if char_count <= 18 {
        return "*".repeat(char_count);
    }
    let prefix: String = token.chars().take(10).collect();
    let suffix: String = token.chars().skip(char_count - 4).collect();
    format!("{prefix}********{suffix}")
}
