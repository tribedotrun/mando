//! Typed message parsing for CC stream-json protocol.

/// A message from the CC subprocess stdout.
#[derive(Debug, Clone)]
pub enum CcMessage {
    Init(InitMessage),
    Assistant(AssistantMessage),
    Result(ResultMessage),
    ControlRequest(ControlRequest),
    RateLimit(RateLimitEvent),
    /// Unrecognized message type — forward compatible.
    Other(serde_json::Value),
}

/// Rate limit status from the CLI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RateLimitStatus {
    Allowed,
    AllowedWarning,
    Rejected,
    Unknown(String),
}

impl RateLimitStatus {
    /// String tag for this status (protocol-level values).
    pub fn as_str(&self) -> &str {
        match self {
            Self::Allowed => "allowed",
            Self::AllowedWarning => "allowed_warning",
            Self::Rejected => "rejected",
            Self::Unknown(v) => v.as_str(),
        }
    }
}

/// Rate limit event emitted when subscription rate limit status changes.
#[derive(Debug, Clone)]
pub struct RateLimitEvent {
    pub status: RateLimitStatus,
    pub resets_at: Option<u64>,
    pub rate_limit_type: Option<String>,
    pub utilization: Option<f64>,
    /// Status of overage/pay-as-you-go usage if applicable.
    pub overage_status: Option<RateLimitStatus>,
    /// Unix timestamp when overage window resets.
    pub overage_resets_at: Option<u64>,
    /// Why overage is unavailable if status is rejected.
    pub overage_disabled_reason: Option<String>,
    pub raw: serde_json::Value,
}

/// System init message — always the first message emitted.
#[derive(Debug, Clone)]
pub struct InitMessage {
    pub session_id: String,
    pub tools: Vec<String>,
    pub model: String,
    pub cwd: String,
    pub raw: serde_json::Value,
}

/// Assistant response with content blocks.
#[derive(Debug, Clone)]
pub struct AssistantMessage {
    pub content: Vec<ContentBlock>,
    pub model: String,
    pub session_id: Option<String>,
    pub uuid: Option<String>,
    /// API message ID (`msg_...`) — distinct from the CC event `uuid`.
    /// Useful for deduplicating re-emitted turns across stream reads.
    pub message_id: Option<String>,
    pub usage: Option<serde_json::Value>,
    /// API error type (e.g. "rate_limit", "server_error") if this turn errored.
    pub error: Option<String>,
    /// Why the model stopped (e.g. "end_turn", "tool_use", "max_tokens").
    pub stop_reason: Option<String>,
    pub raw: serde_json::Value,
}

/// Content block within an assistant message.
#[derive(Debug, Clone)]
pub enum ContentBlock {
    Text {
        text: String,
    },
    ToolUse {
        id: String,
        name: String,
        input: serde_json::Value,
    },
    Thinking {
        text: String,
    },
}

/// Result subtypes — why the loop ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResultSubtype {
    Success,
    ErrorMaxTurns,
    ErrorMaxBudgetUsd,
    ErrorDuringExecution,
    ErrorMaxStructuredOutputRetries,
    Unknown(String),
}

/// Final result message — always the last message.
#[derive(Debug, Clone)]
pub struct ResultMessage {
    pub subtype: ResultSubtype,
    pub is_error: bool,
    pub result_text: String,
    pub structured_output: Option<serde_json::Value>,
    pub session_id: String,
    pub total_cost_usd: Option<f64>,
    pub duration_ms: Option<u64>,
    pub duration_api_ms: Option<u64>,
    pub num_turns: Option<u32>,
    pub usage: Option<serde_json::Value>,
    /// Per-model token/cost breakdown (added in 2026-03 CLI). Raw JSON —
    /// callers needing typed access should parse themselves since schema
    /// continues to evolve. Accepts `model_usage` or `modelUsage` keys.
    pub model_usage: Option<serde_json::Value>,
    /// Count of tool calls denied by the permission system during the session.
    /// Reported by the CLI as either a numeric count or a list of denials —
    /// we store whichever shape was emitted.
    pub permission_denials: Option<serde_json::Value>,
    /// Error strings collected during execution (e.g. API errors, tool failures).
    pub errors: Vec<String>,
    pub raw: serde_json::Value,
}

/// Control request from CLI (hooks, permissions).
#[derive(Debug, Clone)]
pub struct ControlRequest {
    pub request_id: String,
    pub subtype: String,
    pub payload: serde_json::Value,
    pub raw: serde_json::Value,
}

impl CcMessage {
    /// Parse a JSON line from stdout into a typed message.
    ///
    /// Forward-compatible: unknown message types return `CcMessage::Other(val)`
    /// and missing string fields default to the empty string, since the CC
    /// stream-JSON protocol evolves upstream. Both cases are logged at
    /// `debug`/`warn` so protocol drift is observable in the runtime.
    pub fn parse(val: serde_json::Value) -> Self {
        let msg_type = val.get("type").and_then(|t| t.as_str()).unwrap_or("");
        match msg_type {
            "system" => {
                let subtype = val.get("subtype").and_then(|s| s.as_str()).unwrap_or("");
                if subtype == "init" {
                    let session_id = str_field(&val, "session_id");
                    if session_id.is_empty() {
                        tracing::warn!(module = "cc-message", "init message missing session_id");
                    }
                    CcMessage::Init(InitMessage {
                        session_id,
                        tools: val
                            .get("tools")
                            .and_then(|t| t.as_array())
                            .map(|arr| {
                                arr.iter()
                                    .filter_map(|v| v.as_str().map(String::from))
                                    .collect()
                            })
                            .unwrap_or_default(),
                        model: str_field(&val, "model"),
                        cwd: str_field(&val, "cwd"),
                        raw: val,
                    })
                } else {
                    tracing::debug!(
                        module = "cc-message",
                        subtype = %subtype,
                        "unknown system subtype — forwarding as Other"
                    );
                    CcMessage::Other(val)
                }
            }
            "assistant" => {
                let content = parse_content_blocks(&val);
                let model = val
                    .pointer("/message/model")
                    .and_then(|m| m.as_str())
                    .unwrap_or("")
                    .to_string();
                let usage = val.pointer("/message/usage").cloned();
                let stop_reason = val
                    .pointer("/message/stop_reason")
                    .and_then(|s| s.as_str())
                    .map(String::from);
                let error = val.get("error").and_then(|s| s.as_str()).map(String::from);
                let message_id = val
                    .pointer("/message/id")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                CcMessage::Assistant(AssistantMessage {
                    content,
                    model,
                    session_id: val
                        .get("session_id")
                        .and_then(|s| s.as_str())
                        .map(String::from),
                    uuid: val.get("uuid").and_then(|s| s.as_str()).map(String::from),
                    message_id,
                    usage,
                    error,
                    stop_reason,
                    raw: val,
                })
            }
            "result" => {
                let subtype_str = val.get("subtype").and_then(|s| s.as_str()).unwrap_or("");
                if subtype_str.is_empty() {
                    tracing::warn!(module = "cc-message", "result message missing subtype");
                }
                let subtype = match subtype_str {
                    "success" => ResultSubtype::Success,
                    "error_max_turns" => ResultSubtype::ErrorMaxTurns,
                    "error_max_budget_usd" => ResultSubtype::ErrorMaxBudgetUsd,
                    "error_during_execution" => ResultSubtype::ErrorDuringExecution,
                    "error_max_structured_output_retries" => {
                        ResultSubtype::ErrorMaxStructuredOutputRetries
                    }
                    other => ResultSubtype::Unknown(other.to_string()),
                };
                let errors = val
                    .get("errors")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect()
                    })
                    .unwrap_or_default();
                // Accept either snake_case (stream-json protocol) or camelCase
                // (observed in some SDK-emitted events) to stay forward-compat.
                // Filter nulls per-key so a null snake_case value still falls
                // back to a valid camelCase value instead of being dropped.
                let model_usage = val
                    .get("model_usage")
                    .filter(|v| !v.is_null())
                    .or_else(|| val.get("modelUsage").filter(|v| !v.is_null()))
                    .cloned();
                let permission_denials = val
                    .get("permission_denials")
                    .filter(|v| !v.is_null())
                    .or_else(|| val.get("permissionDenials").filter(|v| !v.is_null()))
                    .cloned();
                CcMessage::Result(ResultMessage {
                    subtype,
                    is_error: val
                        .get("is_error")
                        .and_then(|e| e.as_bool())
                        .unwrap_or(false),
                    result_text: str_field(&val, "result"),
                    structured_output: val
                        .get("structured_output")
                        .filter(|v| !v.is_null())
                        .cloned(),
                    session_id: str_field(&val, "session_id"),
                    total_cost_usd: val.get("total_cost_usd").and_then(|v| v.as_f64()),
                    duration_ms: val.get("duration_ms").and_then(|v| v.as_u64()),
                    duration_api_ms: val.get("duration_api_ms").and_then(|v| v.as_u64()),
                    num_turns: val
                        .get("num_turns")
                        .and_then(|v| v.as_u64())
                        .map(|n| n as u32),
                    usage: val.get("usage").cloned(),
                    model_usage,
                    permission_denials,
                    errors,
                    raw: val,
                })
            }
            "control_request" => {
                let request = val
                    .get("request")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                CcMessage::ControlRequest(ControlRequest {
                    request_id: str_field(&val, "request_id"),
                    subtype: request
                        .get("subtype")
                        .and_then(|s| s.as_str())
                        .unwrap_or("")
                        .to_string(),
                    payload: request,
                    raw: val,
                })
            }
            "rate_limit_event" => {
                let info = val
                    .get("rate_limit_info")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                let status_str = info.get("status").and_then(|s| s.as_str()).unwrap_or("");
                let status = match status_str {
                    "allowed" => RateLimitStatus::Allowed,
                    "allowed_warning" => RateLimitStatus::AllowedWarning,
                    "rejected" => RateLimitStatus::Rejected,
                    other => RateLimitStatus::Unknown(other.to_string()),
                };
                let overage_status_str = info.get("overageStatus").and_then(|s| s.as_str());
                let overage_status = overage_status_str.map(|s| match s {
                    "allowed" => RateLimitStatus::Allowed,
                    "allowed_warning" => RateLimitStatus::AllowedWarning,
                    "rejected" => RateLimitStatus::Rejected,
                    other => RateLimitStatus::Unknown(other.to_string()),
                });
                CcMessage::RateLimit(RateLimitEvent {
                    status,
                    resets_at: info.get("resetsAt").and_then(|v| v.as_u64()),
                    rate_limit_type: info
                        .get("rateLimitType")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    utilization: info.get("utilization").and_then(|v| v.as_f64()),
                    overage_status,
                    overage_resets_at: info.get("overageResetsAt").and_then(|v| v.as_u64()),
                    overage_disabled_reason: info
                        .get("overageDisabledReason")
                        .and_then(|v| v.as_str())
                        .map(String::from),
                    raw: val,
                })
            }
            _ => {
                tracing::debug!(
                    module = "cc-message",
                    msg_type = %msg_type,
                    "unknown stream message type — forwarding as Other"
                );
                CcMessage::Other(val)
            }
        }
    }
}

fn str_field(val: &serde_json::Value, key: &str) -> String {
    val.get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

fn parse_content_blocks(val: &serde_json::Value) -> Vec<ContentBlock> {
    let arr = match val.pointer("/message/content").and_then(|c| c.as_array()) {
        Some(a) => a,
        None => return Vec::new(),
    };
    arr.iter()
        .filter_map(|block| {
            let block_type = block.get("type").and_then(|t| t.as_str()).unwrap_or("");
            match block_type {
                "text" => Some(ContentBlock::Text {
                    text: block
                        .get("text")
                        .and_then(|t| t.as_str())
                        .unwrap_or("")
                        .to_string(),
                }),
                "tool_use" => Some(ContentBlock::ToolUse {
                    id: block
                        .get("id")
                        .and_then(|t| t.as_str())
                        .unwrap_or("")
                        .to_string(),
                    name: block
                        .get("name")
                        .and_then(|t| t.as_str())
                        .unwrap_or("")
                        .to_string(),
                    input: block
                        .get("input")
                        .cloned()
                        .unwrap_or(serde_json::Value::Null),
                }),
                "thinking" => Some(ContentBlock::Thinking {
                    text: block
                        .get("text")
                        .and_then(|t| t.as_str())
                        .unwrap_or("")
                        .to_string(),
                }),
                _ => None,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_init_message() {
        let val = serde_json::json!({
            "type": "system",
            "subtype": "init",
            "session_id": "abc-123",
            "tools": ["Read", "Write"],
            "model": "claude-sonnet-4-6",
            "cwd": "/tmp"
        });
        match CcMessage::parse(val) {
            CcMessage::Init(init) => {
                assert_eq!(init.session_id, "abc-123");
                assert_eq!(init.tools, vec!["Read", "Write"]);
                assert_eq!(init.model, "claude-sonnet-4-6");
            }
            other => panic!("expected Init, got {:?}", other),
        }
    }

    #[test]
    fn parse_result_message() {
        let val = serde_json::json!({
            "type": "result",
            "subtype": "success",
            "is_error": false,
            "result": "done",
            "structured_output": {"answer": "42"},
            "session_id": "xyz",
            "total_cost_usd": 0.05,
            "duration_ms": 1234,
            "duration_api_ms": 980,
            "num_turns": 3,
            "errors": ["rate limit retry", "tool timeout"]
        });
        match CcMessage::parse(val) {
            CcMessage::Result(r) => {
                assert_eq!(r.subtype, ResultSubtype::Success);
                assert!(!r.is_error);
                assert_eq!(r.result_text, "done");
                assert!(r.structured_output.is_some());
                assert_eq!(r.total_cost_usd, Some(0.05));
                assert_eq!(r.duration_api_ms, Some(980));
                assert_eq!(r.num_turns, Some(3));
                assert_eq!(r.errors, vec!["rate limit retry", "tool timeout"]);
            }
            other => panic!("expected Result, got {:?}", other),
        }
    }

    #[test]
    fn parse_result_no_errors_field() {
        let val = serde_json::json!({
            "type": "result",
            "subtype": "success",
            "is_error": false,
            "result": "ok",
            "session_id": "abc"
        });
        match CcMessage::parse(val) {
            CcMessage::Result(r) => {
                assert!(r.errors.is_empty());
                assert!(r.duration_api_ms.is_none());
            }
            other => panic!("expected Result, got {:?}", other),
        }
    }

    #[test]
    fn parse_assistant_with_tool_use() {
        let val = serde_json::json!({
            "type": "assistant",
            "message": {
                "model": "claude-sonnet-4-6",
                "content": [
                    {"type": "text", "text": "Let me read that file."},
                    {"type": "tool_use", "id": "tu_1", "name": "Read", "input": {"path": "/tmp/foo"}}
                ]
            },
            "session_id": "abc"
        });
        match CcMessage::parse(val) {
            CcMessage::Assistant(a) => {
                assert_eq!(a.content.len(), 2);
                assert!(
                    matches!(&a.content[0], ContentBlock::Text { text } if text.contains("read"))
                );
                assert!(
                    matches!(&a.content[1], ContentBlock::ToolUse { name, .. } if name == "Read")
                );
            }
            other => panic!("expected Assistant, got {:?}", other),
        }
    }

    #[test]
    fn parse_rate_limit_event() {
        let val = serde_json::json!({
            "type": "rate_limit_event",
            "rate_limit_info": {
                "status": "allowed_warning",
                "resetsAt": 1773273600_u64,
                "rateLimitType": "seven_day",
                "utilization": 0.62
            }
        });
        match CcMessage::parse(val) {
            CcMessage::RateLimit(rl) => {
                assert_eq!(rl.status, RateLimitStatus::AllowedWarning);
                assert_eq!(rl.resets_at, Some(1773273600));
                assert_eq!(rl.rate_limit_type.as_deref(), Some("seven_day"));
                assert!((rl.utilization.unwrap() - 0.62).abs() < 0.01);
            }
            other => panic!("expected RateLimit, got {:?}", other),
        }
    }

    #[test]
    fn parse_rate_limit_rejected() {
        let val = serde_json::json!({
            "type": "rate_limit_event",
            "rate_limit_info": {
                "status": "rejected",
                "resetsAt": 1773273600_u64,
                "rateLimitType": "five_hour",
                "utilization": 1.0
            }
        });
        match CcMessage::parse(val) {
            CcMessage::RateLimit(rl) => {
                assert_eq!(rl.status, RateLimitStatus::Rejected);
            }
            other => panic!("expected RateLimit, got {:?}", other),
        }
    }

    #[test]
    fn parse_rate_limit_with_overage() {
        let val = serde_json::json!({
            "type": "rate_limit_event",
            "rate_limit_info": {
                "status": "rejected",
                "resetsAt": 1773273600_u64,
                "rateLimitType": "five_hour",
                "utilization": 1.0,
                "overageStatus": "allowed_warning",
                "overageResetsAt": 1773280800_u64,
                "overageDisabledReason": null
            }
        });
        match CcMessage::parse(val) {
            CcMessage::RateLimit(rl) => {
                assert_eq!(rl.status, RateLimitStatus::Rejected);
                assert_eq!(rl.overage_status, Some(RateLimitStatus::AllowedWarning));
                assert_eq!(rl.overage_resets_at, Some(1773280800));
                assert!(rl.overage_disabled_reason.is_none());
            }
            other => panic!("expected RateLimit, got {:?}", other),
        }
    }

    #[test]
    fn parse_assistant_with_error_and_stop_reason() {
        let val = serde_json::json!({
            "type": "assistant",
            "message": {
                "model": "claude-opus-4-7",
                "content": [{"type": "text", "text": "error occurred"}],
                "stop_reason": "max_tokens"
            },
            "error": "rate_limit",
            "session_id": "xyz"
        });
        match CcMessage::parse(val) {
            CcMessage::Assistant(a) => {
                assert_eq!(a.error.as_deref(), Some("rate_limit"));
                assert_eq!(a.stop_reason.as_deref(), Some("max_tokens"));
            }
            other => panic!("expected Assistant, got {:?}", other),
        }
    }

    #[test]
    fn parse_result_with_model_usage_and_denials() {
        let val = serde_json::json!({
            "type": "result",
            "subtype": "success",
            "is_error": false,
            "result": "done",
            "session_id": "xyz",
            "total_cost_usd": 0.12,
            "model_usage": {
                "claude-opus-4-7": {
                    "input_tokens": 1500,
                    "output_tokens": 800,
                    "cost_usd": 0.11
                },
                "claude-haiku-4-5": {
                    "input_tokens": 200,
                    "output_tokens": 50,
                    "cost_usd": 0.01
                }
            },
            "permission_denials": [
                {"tool_name": "Bash", "reason": "deny rule: rm -rf"}
            ]
        });
        match CcMessage::parse(val) {
            CcMessage::Result(r) => {
                let mu = r.model_usage.expect("model_usage present");
                assert!(mu.get("claude-opus-4-7").is_some());
                let denials = r
                    .permission_denials
                    .expect("permission_denials present")
                    .as_array()
                    .cloned()
                    .expect("denials is array");
                assert_eq!(denials.len(), 1);
            }
            other => panic!("expected Result, got {:?}", other),
        }
    }

    #[test]
    fn parse_result_null_snake_falls_back_to_camel() {
        // Regression: a present-but-null snake_case value must not mask a
        // valid camelCase value. Upstream sometimes emits both.
        let val = serde_json::json!({
            "type": "result",
            "subtype": "success",
            "session_id": "abc",
            "result": "",
            "model_usage": null,
            "modelUsage": {"claude-opus-4-7": {"cost_usd": 0.07}},
            "permission_denials": null,
            "permissionDenials": 2
        });
        match CcMessage::parse(val) {
            CcMessage::Result(r) => {
                let mu = r.model_usage.expect("model_usage fell back to camel");
                assert!(mu.get("claude-opus-4-7").is_some());
                assert_eq!(
                    r.permission_denials.as_ref().and_then(|v| v.as_u64()),
                    Some(2)
                );
            }
            other => panic!("expected Result, got {:?}", other),
        }
    }

    #[test]
    fn parse_result_accepts_camelcase_keys() {
        // Forward-compat: camelCase keys seen in some SDK-emitted events.
        let val = serde_json::json!({
            "type": "result",
            "subtype": "success",
            "session_id": "abc",
            "result": "",
            "modelUsage": {"claude-opus-4-7": {"cost_usd": 0.05}},
            "permissionDenials": 3
        });
        match CcMessage::parse(val) {
            CcMessage::Result(r) => {
                assert!(r.model_usage.is_some());
                assert_eq!(
                    r.permission_denials.as_ref().and_then(|v| v.as_u64()),
                    Some(3)
                );
            }
            other => panic!("expected Result, got {:?}", other),
        }
    }

    #[test]
    fn parse_assistant_captures_message_id() {
        let val = serde_json::json!({
            "type": "assistant",
            "message": {
                "id": "msg_01ABC",
                "model": "claude-opus-4-7",
                "content": [{"type": "text", "text": "hi"}]
            },
            "session_id": "s1"
        });
        match CcMessage::parse(val) {
            CcMessage::Assistant(a) => {
                assert_eq!(a.message_id.as_deref(), Some("msg_01ABC"));
            }
            other => panic!("expected Assistant, got {:?}", other),
        }
    }

    #[test]
    fn parse_assistant_missing_message_id_ok() {
        let val = serde_json::json!({
            "type": "assistant",
            "message": {
                "model": "claude-opus-4-7",
                "content": []
            },
            "session_id": "s1"
        });
        match CcMessage::parse(val) {
            CcMessage::Assistant(a) => assert!(a.message_id.is_none()),
            other => panic!("expected Assistant, got {:?}", other),
        }
    }

    #[test]
    fn parse_control_request() {
        let val = serde_json::json!({
            "type": "control_request",
            "request_id": "req_1",
            "request": {
                "subtype": "can_use_tool",
                "tool_name": "Bash",
                "input": {"command": "echo hi"}
            }
        });
        match CcMessage::parse(val) {
            CcMessage::ControlRequest(cr) => {
                assert_eq!(cr.request_id, "req_1");
                assert_eq!(cr.subtype, "can_use_tool");
            }
            other => panic!("expected ControlRequest, got {:?}", other),
        }
    }
}
