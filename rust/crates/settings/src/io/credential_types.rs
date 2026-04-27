//! Public types for the credentials table — `CredentialRow`, the public
//! `CredentialInfo` payload, and the helpers that map between them.
//! Split out of `credentials.rs` to keep that file under the file-length
//! budget for query code.

/// A credential row from the database.
///
/// New Codex-only columns (`provider`, `refresh_token`, `id_token`,
/// `account_id`, `plan_type`, `credits_balance`, `credits_unlimited`) come
/// from migration 036. Existing Claude rows have `provider='claude'` (DB
/// default) and `NULL` Codex fields.
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct CredentialRow {
    pub id: i64,
    pub label: String,
    pub access_token: String,
    pub expires_at: Option<i64>, // Unix ms; None = no expiry
    pub rate_limit_cooldown_until: Option<i64>, // Unix seconds
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
    /// Fraction of the window consumed, `[0.0, 1.0]`.
    pub utilization: f64,
    /// Unix seconds when the window resets.
    pub reset_at: i64,
    /// `allowed` / `allowed_warning` / `rejected`.
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
    /// `"claude"` or `"codex"`. Mapped to the `CredentialProvider` enum at
    /// the wire boundary.
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
    /// Accumulated session cost (USD) on this credential since `last_probed_at`.
    /// Summed from `cc_sessions.cost_usd` as a between-probe burn indicator.
    /// Never a substitute for a real probe.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_since_probe_usd: Option<f64>,
    /// Set only when `provider == "codex"`.
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

/// Mask a token: first 10 chars + fixed 8 stars + last 4 chars.
/// Counts by Unicode scalar values so non-ASCII tokens don't panic on byte slicing.
fn mask_token(token: &str) -> String {
    let char_count = token.chars().count();
    if char_count <= 18 {
        return "*".repeat(char_count);
    }
    let prefix: String = token.chars().take(10).collect();
    let suffix: String = token.chars().skip(char_count - 4).collect();
    format!("{prefix}********{suffix}")
}
