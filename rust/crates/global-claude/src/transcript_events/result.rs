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
    let reported_error = val
        .get("is_error")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);
    let outcome = ResultOutcome::from_subtype(raw_subtype, reported_error);
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
        .unwrap_or_else(|| outcome.is_error());
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
