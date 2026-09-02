//! Claude-specific stream fields layered over canonical provider streams.

use std::path::Path;

pub use agent_runtime_core::{
    current_session_lines, get_last_assistant_text, get_stream_file_size, get_stream_result,
    is_clean_result, result_outcome, stream_has_broken_session, stream_stale_seconds,
    write_error_result, write_interrupted_result,
};

/// A rate-limit rejection detected in a CC stream.
pub struct RateLimitRejection {
    /// Unix timestamp (seconds) when the rate-limit window boundary resets.
    pub resets_at: u64,
    /// Which window triggered the rejection (e.g. `"five_hour"`, `"seven_day"`).
    pub rate_limit_type: Option<String>,
}

/// Check if the current session in a stream file contains a rate_limit_event
/// with `rejected` status. Returns rejection details if present.
pub fn has_rate_limit_rejection(stream_path: &Path) -> Option<RateLimitRejection> {
    let (content, last_init_idx) = current_session_lines(stream_path)?;
    let lines: Vec<&str> = content.lines().collect();
    // Scan backwards — the most recent rate_limit_event is authoritative.
    // If it's not rejected (e.g. allowed/allowed_warning), stop immediately
    // rather than scanning older events which may have stale rejections.
    for line in lines[last_init_idx..].iter().rev() {
        let val: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if val.get("type").and_then(|t| t.as_str()) != Some("rate_limit_event") {
            continue;
        }
        let info = match val.get("rate_limit_info") {
            Some(i) => i,
            None => continue,
        };
        // Most recent rate_limit_event found — check and return.
        if info.get("status").and_then(|s| s.as_str()) == Some("rejected") {
            return Some(RateLimitRejection {
                resets_at: info.get("resetsAt").and_then(|v| v.as_u64()).unwrap_or(0),
                rate_limit_type: info
                    .get("rateLimitType")
                    .and_then(|v| v.as_str())
                    .map(String::from),
            });
        }
        return None;
    }
    None
}

/// The most recent rate-limit status from a CC stream (any status).
pub struct StreamRateLimitInfo {
    pub status: String,
    pub resets_at: Option<u64>,
    pub rate_limit_type: Option<String>,
    pub utilization: Option<f64>,
    pub overage_status: Option<String>,
}

/// Read the most recent `rate_limit_event` from a stream file, regardless of
/// status. Returns `None` if the stream has no rate-limit events.
pub fn last_rate_limit_status(stream_path: &Path) -> Option<StreamRateLimitInfo> {
    let (content, last_init_idx) = current_session_lines(stream_path)?;
    let lines: Vec<&str> = content.lines().collect();
    for line in lines[last_init_idx..].iter().rev() {
        let val: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if val.get("type").and_then(|t| t.as_str()) != Some("rate_limit_event") {
            continue;
        }
        let info = match val.get("rate_limit_info") {
            Some(i) => i,
            None => continue,
        };
        return Some(StreamRateLimitInfo {
            status: info
                .get("status")
                .and_then(|s| s.as_str())
                .unwrap_or("unknown")
                .to_string(),
            resets_at: info.get("resetsAt").and_then(|v| v.as_u64()),
            rate_limit_type: info
                .get("rateLimitType")
                .and_then(|v| v.as_str())
                .map(String::from),
            utilization: info.get("utilization").and_then(|v| v.as_f64()),
            overage_status: info
                .get("overageStatus")
                .and_then(|v| v.as_str())
                .map(String::from),
        });
    }
    None
}

/// Cost, duration, turn count, and session health metrics extracted from a
/// stream result event.
pub struct StreamCostInfo {
    pub cost_usd: Option<f64>,
    pub duration_ms: Option<u64>,
    pub num_turns: Option<i64>,
    /// Number of tool invocations blocked by the permission system. Coerced to
    /// a count whether the CLI emitted a numeric field or a list of denials.
    pub permission_denials_count: Option<u64>,
    /// Raw per-model token/cost breakdown as emitted by the CLI. Kept as JSON
    /// because the inner schema still evolves upstream.
    pub model_usage: Option<serde_json::Value>,
}

/// Lifetime totals across every completed Claude invocation in one JSONL
/// file. A resumed Claude session appends a new `init` and a new per-invocation
/// `result`; those result values are deltas and must be summed.
#[derive(Debug, Clone, PartialEq)]
pub struct StreamCostTotals {
    pub cost_usd: Option<f64>,
    pub duration_ms: Option<u64>,
    pub num_turns: Option<i64>,
    pub segment_count: u32,
}

/// Read either `permission_denials` or `permissionDenials`, then coerce to a count.
///
/// Filter nulls per-key so a null snake_case value still falls back to a
/// valid camelCase value instead of being dropped.
fn permission_denials_count(result: &serde_json::Value) -> Option<u64> {
    let raw = result
        .get("permission_denials")
        .filter(|v| !v.is_null())
        .or_else(|| result.get("permissionDenials").filter(|v| !v.is_null()))?;
    if let Some(n) = raw.as_u64() {
        return Some(n);
    }
    if let Some(arr) = raw.as_array() {
        return Some(arr.len() as u64);
    }
    None
}

/// Extract cost, duration, turns, and session-health fields from the result
/// event in a JSONL stream file.
///
/// Returns `None` if the stream file is missing or has no result event.
pub fn get_stream_cost(stream_path: &Path) -> Option<StreamCostInfo> {
    let result = get_stream_result(stream_path)?;
    let model_usage = result
        .get("model_usage")
        .filter(|v| !v.is_null())
        .or_else(|| result.get("modelUsage").filter(|v| !v.is_null()))
        .cloned();
    Some(StreamCostInfo {
        cost_usd: result.get("total_cost_usd").and_then(|v| v.as_f64()),
        duration_ms: result.get("duration_ms").and_then(|v| v.as_u64()),
        num_turns: result.get("num_turns").and_then(|v| v.as_i64()),
        permission_denials_count: permission_denials_count(&result),
        model_usage,
    })
}

/// Sum every Claude `type:result` envelope in a stream. Unlike
/// [`get_stream_cost`], this intentionally crosses resume boundaries.
pub fn get_stream_cost_totals(stream_path: &Path) -> Option<StreamCostTotals> {
    let content = match std::fs::read_to_string(stream_path) {
        Ok(content) => content,
        Err(error) => {
            tracing::debug!(
                path = %stream_path.display(),
                error = %error,
                "cannot read Claude stream for lifetime totals",
            );
            return None;
        }
    };
    let mut totals = StreamCostTotals {
        cost_usd: None,
        duration_ms: None,
        num_turns: None,
        segment_count: 0,
    };
    for line in content.lines() {
        let value: serde_json::Value = match serde_json::from_str(line) {
            Ok(value) => value,
            Err(_) => continue,
        };
        if value.get("type").and_then(|v| v.as_str()) != Some("result") {
            continue;
        }
        totals.segment_count = totals.segment_count.saturating_add(1);
        if let Some(cost) = value.get("total_cost_usd").and_then(|v| v.as_f64()) {
            totals.cost_usd = Some(totals.cost_usd.unwrap_or(0.0) + cost);
        }
        if let Some(duration) = value.get("duration_ms").and_then(|v| v.as_u64()) {
            totals.duration_ms = Some(totals.duration_ms.unwrap_or(0).saturating_add(duration));
        }
        if let Some(turns) = value.get("num_turns").and_then(|v| v.as_i64()) {
            totals.num_turns = Some(totals.num_turns.unwrap_or(0).saturating_add(turns));
        }
    }
    (totals.segment_count > 0).then_some(totals)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_result_current_session() {
        let dir = std::env::temp_dir().join("mando-cc-test-stream");
        std::fs::create_dir_all(&dir).ok();
        let path = dir.join("test.jsonl");

        let content = [
            r#"{"type":"system","subtype":"init"}"#,
            r#"{"type":"result","subtype":"success","result":"old"}"#,
            r#"{"type":"system","subtype":"init"}"#,
            r#"{"type":"result","subtype":"success","result":"current"}"#,
        ]
        .join("\n");
        std::fs::write(&path, &content).unwrap();

        let result = get_stream_result(&path).unwrap();
        assert_eq!(result["result"].as_str(), Some("current"));

        std::fs::remove_file(&path).ok();
        std::fs::remove_dir(&dir).ok();
    }

    #[test]
    fn stream_result_no_result_in_current() {
        let dir = std::env::temp_dir().join("mando-cc-test-noresult");
        std::fs::create_dir_all(&dir).ok();
        let path = dir.join("test.jsonl");

        let content = [
            r#"{"type":"system","subtype":"init"}"#,
            r#"{"type":"result","subtype":"success"}"#,
            r#"{"type":"system","subtype":"init"}"#,
            r#"{"type":"assistant","message":"working"}"#,
        ]
        .join("\n");
        std::fs::write(&path, &content).unwrap();

        assert!(get_stream_result(&path).is_none());

        std::fs::remove_file(&path).ok();
        std::fs::remove_dir(&dir).ok();
    }

    #[test]
    fn clean_result_success() {
        assert!(is_clean_result(&serde_json::json!({"subtype": "success"})));
    }

    #[test]
    fn clean_result_error() {
        assert!(!is_clean_result(
            &serde_json::json!({"subtype": "error_max_turns"})
        ));
    }

    #[test]
    fn interrupted_result_is_terminal_but_not_clean() {
        let result = serde_json::json!({
            "subtype": "interrupted",
            "is_error": false
        });

        assert_eq!(
            result_outcome(&result),
            api_types::ResultOutcome::Interrupted
        );
        assert!(!is_clean_result(&result));
    }

    #[test]
    fn clean_result_is_error_wins_over_success_subtype() {
        assert!(!is_clean_result(&serde_json::json!({
            "subtype": "success",
            "is_error": true
        })));
    }

    #[test]
    fn synthetic_interrupted_result_uses_canonical_outcome() {
        let file = tempfile::NamedTempFile::new().unwrap();

        write_interrupted_result(file.path());

        let result = get_stream_result(file.path()).unwrap();
        assert_eq!(
            result_outcome(&result),
            api_types::ResultOutcome::Interrupted
        );
        assert_eq!(result["is_error"], serde_json::json!(false));
    }

    #[test]
    fn broken_session_detection() {
        let dir = std::env::temp_dir().join("mando-cc-test-broken");
        std::fs::create_dir_all(&dir).ok();
        let path = dir.join("broken.jsonl");

        // Content but no init event = broken.
        std::fs::write(&path, r#"{"type":"assistant","message":"hi"}"#).unwrap();
        assert!(stream_has_broken_session(&path));

        // With init = not broken.
        std::fs::write(&path, r#"{"type":"system","subtype":"init"}"#).unwrap();
        assert!(!stream_has_broken_session(&path));

        std::fs::remove_file(&path).ok();
        std::fs::remove_dir(&dir).ok();
    }

    #[test]
    fn file_size_missing() {
        // Use a dynamically constructed path guaranteed not to exist on the
        // test host. The previous `/nonexistent` literal accidentally matched
        // a real directory in some sandboxed CI environments.
        let missing =
            std::env::temp_dir().join(format!("mando-cc-missing-{}.jsonl", std::process::id()));
        assert_eq!(get_stream_file_size(&missing), 0);
    }

    #[test]
    fn stream_cost_with_duration() {
        let dir = std::env::temp_dir().join("mando-cc-test-cost-dur");
        std::fs::create_dir_all(&dir).ok();
        let path = dir.join("test.jsonl");

        let content = [
            r#"{"type":"system","subtype":"init"}"#,
            r#"{"type":"result","subtype":"success","total_cost_usd":0.05,"duration_ms":12345}"#,
        ]
        .join("\n");
        std::fs::write(&path, &content).unwrap();

        let info = get_stream_cost(&path).unwrap();
        assert_eq!(info.cost_usd, Some(0.05));
        assert_eq!(info.duration_ms, Some(12345));

        std::fs::remove_file(&path).ok();
        std::fs::remove_dir(&dir).ok();
    }

    #[test]
    fn stream_cost_totals_sum_resumed_claude_results() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let content = [
            r#"{"type":"system","subtype":"init"}"#,
            r#"{"type":"result","duration_ms":1403447,"num_turns":143,"total_cost_usd":13.45703975}"#,
            r#"{"type":"system","subtype":"init"}"#,
            r#"{"type":"result","duration_ms":18930,"num_turns":3,"total_cost_usd":5.18996975}"#,
        ]
        .join("\n");
        std::fs::write(file.path(), content).unwrap();

        let totals = get_stream_cost_totals(file.path()).unwrap();
        assert_eq!(totals.duration_ms, Some(1_422_377));
        assert_eq!(totals.num_turns, Some(146));
        assert_eq!(totals.segment_count, 2);
        assert!((totals.cost_usd.unwrap_or_default() - 18.6470095).abs() < f64::EPSILON);
    }

    #[test]
    fn stream_cost_missing_duration() {
        let dir = std::env::temp_dir().join("mando-cc-test-cost-nodur");
        std::fs::create_dir_all(&dir).ok();
        let path = dir.join("test.jsonl");

        let content = [
            r#"{"type":"system","subtype":"init"}"#,
            r#"{"type":"result","subtype":"success","total_cost_usd":0.03}"#,
        ]
        .join("\n");
        std::fs::write(&path, &content).unwrap();

        let info = get_stream_cost(&path).unwrap();
        assert_eq!(info.cost_usd, Some(0.03));
        assert!(info.duration_ms.is_none());

        std::fs::remove_file(&path).ok();
        std::fs::remove_dir(&dir).ok();
    }

    #[test]
    fn stream_cost_captures_denials_and_model_usage() {
        let dir = std::env::temp_dir().join("mando-cc-test-denials");
        std::fs::create_dir_all(&dir).ok();
        let path = dir.join("test.jsonl");

        let content = [
            r#"{"type":"system","subtype":"init"}"#,
            r#"{"type":"result","subtype":"success","total_cost_usd":0.2,"permission_denials":[{"tool":"Bash"}],"model_usage":{"claude-opus-4-7":{"cost_usd":0.2}}}"#,
        ]
        .join("\n");
        std::fs::write(&path, &content).unwrap();

        let info = get_stream_cost(&path).unwrap();
        assert_eq!(info.permission_denials_count, Some(1));
        assert!(info.model_usage.is_some());
        assert!(info
            .model_usage
            .as_ref()
            .unwrap()
            .get("claude-opus-4-7")
            .is_some());

        std::fs::remove_file(&path).ok();
        std::fs::remove_dir(&dir).ok();
    }

    #[test]
    fn stream_cost_denials_as_count() {
        let dir = std::env::temp_dir().join("mando-cc-test-denials-count");
        std::fs::create_dir_all(&dir).ok();
        let path = dir.join("test.jsonl");

        // Older CLI shape: numeric count rather than list.
        let content = [
            r#"{"type":"system","subtype":"init"}"#,
            r#"{"type":"result","subtype":"success","permissionDenials":5}"#,
        ]
        .join("\n");
        std::fs::write(&path, &content).unwrap();

        let info = get_stream_cost(&path).unwrap();
        assert_eq!(info.permission_denials_count, Some(5));

        std::fs::remove_file(&path).ok();
        std::fs::remove_dir(&dir).ok();
    }

    #[test]
    fn stream_cost_no_result_event() {
        let dir = std::env::temp_dir().join("mando-cc-test-cost-noresult");
        std::fs::create_dir_all(&dir).ok();
        let path = dir.join("test.jsonl");

        let content = [
            r#"{"type":"system","subtype":"init"}"#,
            r#"{"type":"assistant","message":"working"}"#,
        ]
        .join("\n");
        std::fs::write(&path, &content).unwrap();

        assert!(get_stream_cost(&path).is_none());

        std::fs::remove_file(&path).ok();
        std::fs::remove_dir(&dir).ok();
    }
}
