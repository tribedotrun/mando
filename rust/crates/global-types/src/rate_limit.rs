//! Canonical rate-limit status enum.
//!
//! Two wire protocols surface rate-limit status to Mando: Claude Code's
//! stream-json `rate_limit_event` and the Anthropic Messages API's
//! `anthropic-ratelimit-unified-*-status` response header. Both use the
//! same three string tags (`allowed`, `allowed_warning`, `rejected`).
//! `Unknown(String)` captures any future upstream-added tag so unknown
//! values propagate through the pipeline instead of being silently
//! dropped or panicking.
//!
//! Prior to this type there were two separate `RateLimitStatus` enums in
//! `global-claude` and `settings::usage_probe`; both now re-export
//! this one.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RateLimitStatus {
    Allowed,
    AllowedWarning,
    Rejected,
    /// Forward-compatible escape hatch for future upstream tags. Serialized
    /// as the inner string so round-trip stays stable even for unknowns.
    #[serde(untagged)]
    Unknown(String),
}

impl RateLimitStatus {
    /// String tag. For `Unknown(s)`, returns the inner string.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Allowed => "allowed",
            Self::AllowedWarning => "allowed_warning",
            Self::Rejected => "rejected",
            Self::Unknown(v) => v.as_str(),
        }
    }

    /// Parse from a wire tag. Unknown tags become `Unknown(s)`.
    pub fn parse(s: &str) -> Self {
        match s {
            "allowed" => Self::Allowed,
            "allowed_warning" => Self::AllowedWarning,
            "rejected" => Self::Rejected,
            other => Self::Unknown(other.to_string()),
        }
    }

    /// Returns `true` if status is one of the three canonical values —
    /// false if it is an upstream-introduced unknown tag.
    pub fn is_known(&self) -> bool {
        !matches!(self, Self::Unknown(_))
    }
}
