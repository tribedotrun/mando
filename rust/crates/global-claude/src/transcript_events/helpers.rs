//! Small parsing utilities shared across the event projection submodules.

use api_types::{CcPermissionMode, TranscriptUsageInfo};

pub(super) fn string_array(val: Option<&serde_json::Value>) -> Vec<String> {
    val.and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|entry| entry.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

pub(super) fn parse_permission_mode(s: &str) -> Option<CcPermissionMode> {
    match s {
        "default" => Some(CcPermissionMode::Default),
        "acceptEdits" => Some(CcPermissionMode::AcceptEdits),
        "bypassPermissions" => Some(CcPermissionMode::BypassPermissions),
        "plan" => Some(CcPermissionMode::Plan),
        "dontAsk" => Some(CcPermissionMode::DontAsk),
        "auto" => Some(CcPermissionMode::Auto),
        _ => None,
    }
}

pub(super) fn parse_usage(u: &serde_json::Value) -> TranscriptUsageInfo {
    TranscriptUsageInfo {
        input_tokens: token_field(u, "input_tokens", "inputTokens"),
        output_tokens: token_field(u, "output_tokens", "outputTokens"),
        cache_read_tokens: u
            .get("cache_read_input_tokens")
            .or_else(|| u.get("cacheReadInputTokens"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
        cache_creation_tokens: u
            .get("cache_creation_input_tokens")
            .or_else(|| u.get("cacheCreationInputTokens"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0),
    }
}

fn token_field(u: &serde_json::Value, snake: &str, camel: &str) -> u64 {
    u.get(snake)
        .or_else(|| u.get(camel))
        .and_then(|v| v.as_u64())
        .unwrap_or(0)
}

pub(super) fn str_field(input: &serde_json::Value, key: &str) -> String {
    input
        .get(key)
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string()
}

pub(super) fn opt_str(input: &serde_json::Value, key: &str) -> Option<String> {
    input.get(key).and_then(|v| v.as_str()).map(String::from)
}

pub(super) fn opt_string_array(val: Option<&serde_json::Value>) -> Option<Vec<String>> {
    let arr = val?.as_array()?;
    Some(
        arr.iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect(),
    )
}
