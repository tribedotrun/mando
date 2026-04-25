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

/// Why CC stopped. Mirrors CC's `result.subtype` but with named variants.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ResultOutcome {
    Success,
    ErrorDuringExecution,
    ErrorMaxTurns,
    ErrorMaxBudgetUsd,
    ErrorMaxStructuredOutputRetries,
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
