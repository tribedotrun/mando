//! Tool progress + session-result + unknown catch-all events.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{EventMeta, ToolName};
use crate::TranscriptUsageInfo;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ToolProgressEvent {
    pub meta: EventMeta,
    pub tool_use_id: String,
    pub tool_name: ToolName,
    #[ts(type = "number | null")]
    pub elapsed_seconds: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResultEvent {
    pub meta: EventMeta,
    pub outcome: ResultOutcome,
    pub summary: ResultSummary,
}

/// Why an agent session stopped, normalized across providers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ResultOutcome {
    Success,
    Interrupted,
    ErrorDuringExecution,
    ErrorMaxTurns,
    ErrorMaxBudgetUsd,
    ErrorMaxStructuredOutputRetries,
}

impl ResultOutcome {
    pub fn from_subtype(subtype: Option<&str>, is_error: bool) -> Self {
        match subtype {
            Some("error_max_turns") => Self::ErrorMaxTurns,
            Some("error_max_budget_usd") => Self::ErrorMaxBudgetUsd,
            Some("error_max_structured_output_retries") => Self::ErrorMaxStructuredOutputRetries,
            Some("interrupted") if !is_error => Self::Interrupted,
            Some("success") if !is_error => Self::Success,
            _ => Self::ErrorDuringExecution,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Interrupted => "interrupted",
            Self::ErrorDuringExecution => "error_during_execution",
            Self::ErrorMaxTurns => "error_max_turns",
            Self::ErrorMaxBudgetUsd => "error_max_budget_usd",
            Self::ErrorMaxStructuredOutputRetries => "error_max_structured_output_retries",
        }
    }

    pub const fn is_clean(self) -> bool {
        matches!(self, Self::Success)
    }

    pub const fn is_error(self) -> bool {
        matches!(
            self,
            Self::ErrorDuringExecution
                | Self::ErrorMaxTurns
                | Self::ErrorMaxBudgetUsd
                | Self::ErrorMaxStructuredOutputRetries
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResultSummary {
    #[ts(type = "number | null")]
    pub duration_ms: Option<u64>,
    #[ts(type = "number | null")]
    pub duration_api_ms: Option<u64>,
    #[ts(type = "number | null")]
    pub num_turns: Option<u32>,
    pub total_cost_usd: Option<f64>,
    pub stop_reason: Option<String>,
    pub permission_denials: Vec<PermissionDenial>,
    pub errors: Vec<String>,
    pub usage: Option<TranscriptUsageInfo>,
    pub model_usage: Vec<ModelUsageBreakdown>,
    pub is_error: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PermissionDenial {
    pub tool_name: Option<String>,
    pub tool_use_id: Option<String>,
    pub reason: Option<String>,
}

// Per-turn token counts reuse `TranscriptUsageInfo` from `sessions` — the
// shape is identical and keeping one definition means callers do not have to
// switch between names.

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelUsageBreakdown {
    pub model: String,
    pub usage: TranscriptUsageInfo,
    pub cost_usd: Option<f64>,
    #[ts(type = "number | null")]
    pub context_window: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UnknownEvent {
    pub meta: EventMeta,
    pub raw_type: Option<String>,
    pub raw_subtype: Option<String>,
    pub raw: String,
}

#[cfg(test)]
mod tests {
    use super::ResultOutcome;

    #[test]
    fn canonical_outcome_distinguishes_success_interruption_and_error() {
        assert_eq!(
            ResultOutcome::from_subtype(Some("success"), false),
            ResultOutcome::Success
        );
        assert_eq!(
            ResultOutcome::from_subtype(Some("interrupted"), false),
            ResultOutcome::Interrupted
        );
        assert_eq!(
            ResultOutcome::from_subtype(Some("success"), true),
            ResultOutcome::ErrorDuringExecution
        );
        assert_eq!(
            ResultOutcome::from_subtype(None, false),
            ResultOutcome::ErrorDuringExecution
        );
        assert!(ResultOutcome::Success.is_clean());
        assert!(!ResultOutcome::Interrupted.is_clean());
        assert!(!ResultOutcome::Interrupted.is_error());
    }
}
