//! Result-event parsing (success / error variants, usage, model breakdown).

use api_types::{
    EventMeta, ModelUsageBreakdown, PermissionDenial, ResultEvent, ResultOutcome, ResultSummary,
};

use crate::transcript_events::helpers::parse_usage;

pub(super) fn parse_result(
    val: &serde_json::Value,
    meta: EventMeta,
    raw_subtype: Option<&str>,
) -> ResultEvent {
    let outcome = match raw_subtype.unwrap_or("success") {
        "success" => ResultOutcome::Success,
        "error_max_turns" => ResultOutcome::ErrorMaxTurns,
        "error_max_budget_usd" => ResultOutcome::ErrorMaxBudgetUsd,
        "error_max_structured_output_retries" => ResultOutcome::ErrorMaxStructuredOutputRetries,
        _ => ResultOutcome::ErrorDuringExecution,
    };
    let usage = val.get("usage").map(parse_usage);
    let model_usage = val
        .get("modelUsage")
        .or_else(|| val.get("model_usage"))
        .and_then(|v| v.as_object())
        .map(|obj| {
            obj.iter()
                .map(|(model, payload)| ModelUsageBreakdown {
                    model: model.clone(),
                    usage: parse_usage(payload),
                    cost_usd: payload.get("costUSD").and_then(|v| v.as_f64()),
                    context_window: payload.get("contextWindow").and_then(|v| v.as_u64()),
                })
                .collect()
        })
        .unwrap_or_default();
    let permission_denials = val
        .get("permission_denials")
        .or_else(|| val.get("permissionDenials"))
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().map(parse_permission_denial).collect())
        .unwrap_or_default();
    let errors = val
        .get("errors")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|e| e.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let is_error = val
        .get("is_error")
        .and_then(|v| v.as_bool())
        .unwrap_or(matches!(
            outcome,
            ResultOutcome::ErrorDuringExecution
                | ResultOutcome::ErrorMaxTurns
                | ResultOutcome::ErrorMaxBudgetUsd
                | ResultOutcome::ErrorMaxStructuredOutputRetries
        ));
    let summary = ResultSummary {
        duration_ms: val.get("duration_ms").and_then(|v| v.as_u64()),
        duration_api_ms: val.get("duration_api_ms").and_then(|v| v.as_u64()),
        num_turns: val
            .get("num_turns")
            .and_then(|v| v.as_u64())
            .map(|n| n as u32),
        total_cost_usd: val.get("total_cost_usd").and_then(|v| v.as_f64()),
        stop_reason: val
            .get("stop_reason")
            .and_then(|v| v.as_str())
            .map(String::from),
        permission_denials,
        errors,
        usage,
        model_usage,
        is_error,
    };
    ResultEvent {
        meta,
        outcome,
        summary,
    }
}

fn parse_permission_denial(entry: &serde_json::Value) -> PermissionDenial {
    PermissionDenial {
        tool_name: entry
            .get("tool_name")
            .or_else(|| entry.get("toolName"))
            .or_else(|| entry.get("tool"))
            .and_then(|v| v.as_str())
            .map(String::from),
        tool_use_id: entry
            .get("tool_use_id")
            .or_else(|| entry.get("toolUseId"))
            .and_then(|v| v.as_str())
            .map(String::from),
        reason: entry
            .get("reason")
            .or_else(|| entry.get("message"))
            .and_then(|v| v.as_str())
            .map(String::from),
    }
}
