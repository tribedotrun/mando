//! Structured transcript reading from CC session JSONL files.
//!
//! Parses raw JSONL stream files into typed messages, tool usage summaries,
//! and cost breakdowns. Follows the parentUuid chain for correct ordering.

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// A parsed message from a session transcript.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptMessage {
    pub role: String, // "user" or "assistant"
    pub uuid: String,
    pub parent_uuid: Option<String>,
    pub text: String,
    pub tool_calls: Vec<ToolCall>,
    pub usage: Option<UsageInfo>,
}

/// A tool call within an assistant message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub input_summary: String,
}

/// Token usage for a single turn.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageInfo {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
}

/// Aggregated tool usage for a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolUsageSummary {
    pub name: String,
    pub call_count: u32,
    pub error_count: u32,
}

/// Cost breakdown for a session.
///
/// `total_cost_usd` is populated from the stream's `type:result` envelope
/// when CC emits one (clean `end_turn` exit). On abnormal exits (watchdog
/// abort, process kill, rate-limit abort, any `stop_reason` other than
/// `end_turn`) the result envelope is absent and `total_cost_usd` is
/// `None` — callers that need a numeric fallback should use
/// [`session_cost_or_estimate`] or [`SessionCost::estimated_cost_usd`]
/// which fills in an estimate from per-model usage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionCost {
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cache_read_tokens: u64,
    pub total_cache_creation_tokens: u64,
    pub turn_count: u32,
    pub total_cost_usd: Option<f64>,
    /// Token usage broken down by model so cost estimation can apply
    /// per-model rates. Key is the model string CC reports on each
    /// assistant turn (e.g. `claude-opus-4-7`).
    pub per_model_usage: HashMap<String, ModelUsage>,
}

/// Per-model token totals captured during session-cost aggregation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModelUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_creation_tokens: u64,
}

impl SessionCost {
    /// Estimate cost in USD from per-model token usage using the pricing
    /// table in [`crate::pricing`]. Returns `0.0` when no usage was recorded
    /// (empty session or parse failure).
    pub fn estimated_cost_usd(&self) -> f64 {
        if self.per_model_usage.is_empty() {
            return 0.0;
        }
        self.per_model_usage
            .iter()
            .map(|(model, u)| {
                crate::pricing::rate_for_model(model).cost_for(
                    u.input_tokens,
                    u.output_tokens,
                    u.cache_creation_tokens,
                    u.cache_read_tokens,
                )
            })
            .sum()
    }
}

/// Parse messages from a session JSONL file.
///
/// Returns messages in chronological order, filtering to user/assistant only.
/// Handles session boundaries (only reads from the last init event).
pub fn parse_messages(
    stream_path: &Path,
    limit: Option<usize>,
    offset: usize,
) -> Vec<TranscriptMessage> {
    let (content, last_init_idx) = match crate::stream::current_session_lines(stream_path) {
        Some(c) => c,
        None => return Vec::new(),
    };

    let lines: Vec<&str> = content.lines().collect();
    let mut messages = Vec::new();

    for line in &lines[last_init_idx..] {
        let val: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => {
                tracing::debug!(error = %e, "skipping malformed JSONL line in transcript");
                continue;
            }
        };

        let msg_type = val.get("type").and_then(|t| t.as_str()).unwrap_or("");
        if msg_type != "user" && msg_type != "assistant" {
            continue;
        }

        // Skip sidechains and meta entries.
        if val
            .get("isSidechain")
            .and_then(|s| s.as_bool())
            .unwrap_or(false)
        {
            continue;
        }
        if val.get("isMeta").and_then(|s| s.as_bool()).unwrap_or(false) {
            continue;
        }

        let uuid = val
            .get("uuid")
            .and_then(|u| u.as_str())
            .unwrap_or("")
            .to_string();
        let parent_uuid = val
            .get("parentUuid")
            .and_then(|u| u.as_str())
            .map(String::from);

        let (text, tool_calls) = if msg_type == "assistant" {
            extract_assistant_content(&val)
        } else {
            let text = val
                .pointer("/message/content")
                .and_then(|c| c.as_str())
                .unwrap_or("")
                .to_string();
            (text, Vec::new())
        };

        let usage = val.pointer("/message/usage").map(|u| UsageInfo {
            input_tokens: u.get("input_tokens").and_then(|t| t.as_u64()).unwrap_or(0),
            output_tokens: u.get("output_tokens").and_then(|t| t.as_u64()).unwrap_or(0),
            cache_read_tokens: u
                .get("cache_read_input_tokens")
                .and_then(|t| t.as_u64())
                .unwrap_or(0),
            cache_creation_tokens: u
                .get("cache_creation_input_tokens")
                .and_then(|t| t.as_u64())
                .unwrap_or(0),
        });

        messages.push(TranscriptMessage {
            role: msg_type.to_string(),
            uuid,
            parent_uuid,
            text,
            tool_calls,
            usage,
        });
    }

    // Apply pagination.
    let start = offset.min(messages.len());
    let end = limit
        .map(|l| (start + l).min(messages.len()))
        .unwrap_or(messages.len());
    messages[start..end].to_vec()
}

/// Get aggregated tool usage for a session.
///
/// Scans all lines in the stream (not just the last resume segment) so the
/// totals reflect the full session lifetime.
pub fn tool_usage(stream_path: &Path) -> Vec<ToolUsageSummary> {
    let content = match std::fs::read_to_string(stream_path) {
        Ok(c) => c,
        Err(e) => {
            tracing::debug!(path = %stream_path.display(), error = %e, "cannot read stream file for tool usage");
            return Vec::new();
        }
    };

    let mut tools: HashMap<String, (u32, u32)> = HashMap::new();
    // Reverse lookup: tool_use_id → tool name (for attributing errors).
    let mut tool_use_id_to_name: HashMap<String, String> = HashMap::new();

    for line in content.lines() {
        let val: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => {
                tracing::debug!(error = %e, "skipping malformed JSONL line in tool stats");
                continue;
            }
        };

        let msg_type = val.get("type").and_then(|t| t.as_str()).unwrap_or("");

        // Count tool_use blocks in assistant messages + build reverse lookup.
        if msg_type == "assistant" {
            if let Some(content_arr) = val.pointer("/message/content").and_then(|c| c.as_array()) {
                for block in content_arr {
                    if block.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                        let name = block
                            .get("name")
                            .and_then(|n| n.as_str())
                            .unwrap_or("unknown")
                            .to_string();
                        let id = block
                            .get("id")
                            .and_then(|i| i.as_str())
                            .unwrap_or("")
                            .to_string();
                        tools.entry(name.clone()).or_insert((0, 0)).0 += 1;
                        if !id.is_empty() {
                            tool_use_id_to_name.insert(id, name);
                        }
                    }
                }
            }
        }

        // Count tool_result errors and attribute to correct tool via reverse lookup.
        if msg_type == "user" {
            if let Some(content_arr) = val.pointer("/message/content").and_then(|c| c.as_array()) {
                for block in content_arr {
                    if block.get("type").and_then(|t| t.as_str()) == Some("tool_result")
                        && block
                            .get("is_error")
                            .and_then(|e| e.as_bool())
                            .unwrap_or(false)
                    {
                        let tool_use_id = block
                            .get("tool_use_id")
                            .and_then(|id| id.as_str())
                            .unwrap_or("");
                        if let Some(name) = tool_use_id_to_name.get(tool_use_id) {
                            tools.entry(name.clone()).or_insert((0, 0)).1 += 1;
                        }
                    }
                }
            }
        }
    }

    let mut result: Vec<_> = tools
        .into_iter()
        .map(|(name, (calls, errors))| ToolUsageSummary {
            name,
            call_count: calls,
            error_count: errors,
        })
        .collect();
    result.sort_by(|a, b| b.call_count.cmp(&a.call_count));
    result
}

/// Get cost breakdown for a session.
///
/// Aggregates per-turn `usage` fields (input, output, cache-read,
/// cache-creation) from every `assistant` message in the current-session
/// segment of the stream, plus per-model breakdowns so callers can estimate
/// cost when the authoritative `total_cost_usd` from CC's `type:result`
/// envelope is absent (watchdog abort, abnormal exit).
pub fn session_cost(stream_path: &Path) -> SessionCost {
    let empty = || SessionCost {
        total_input_tokens: 0,
        total_output_tokens: 0,
        total_cache_read_tokens: 0,
        total_cache_creation_tokens: 0,
        turn_count: 0,
        total_cost_usd: None,
        per_model_usage: HashMap::new(),
    };

    let (content, last_init_idx) = match crate::stream::current_session_lines(stream_path) {
        Some(c) => c,
        None => return empty(),
    };

    let lines: Vec<&str> = content.lines().collect();
    let mut cost = empty();

    for line in &lines[last_init_idx..] {
        let val: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => {
                tracing::debug!(error = %e, "skipping malformed JSONL line in cost parse");
                continue;
            }
        };

        let msg_type = val.get("type").and_then(|t| t.as_str()).unwrap_or("");

        if msg_type == "assistant" {
            cost.turn_count += 1;
            // Grab the model from the message envelope so per-model
            // estimation in `estimated_cost_usd` can apply the right rate.
            // Absent / blank model names bucket under the empty string,
            // which `rate_for_model` routes to its opus fallback.
            let model = val
                .pointer("/message/model")
                .and_then(|m| m.as_str())
                .unwrap_or("")
                .to_string();
            if let Some(usage) = val.pointer("/message/usage") {
                let input = usage
                    .get("input_tokens")
                    .and_then(|t| t.as_u64())
                    .unwrap_or(0);
                let output = usage
                    .get("output_tokens")
                    .and_then(|t| t.as_u64())
                    .unwrap_or(0);
                let cache_read = usage
                    .get("cache_read_input_tokens")
                    .and_then(|t| t.as_u64())
                    .unwrap_or(0);
                let cache_creation = usage
                    .get("cache_creation_input_tokens")
                    .and_then(|t| t.as_u64())
                    .unwrap_or(0);

                cost.total_input_tokens += input;
                cost.total_output_tokens += output;
                cost.total_cache_read_tokens += cache_read;
                cost.total_cache_creation_tokens += cache_creation;

                let entry = cost.per_model_usage.entry(model).or_default();
                entry.input_tokens += input;
                entry.output_tokens += output;
                entry.cache_read_tokens += cache_read;
                entry.cache_creation_tokens += cache_creation;
            }
        }

        if msg_type == "result" {
            cost.total_cost_usd = val.get("total_cost_usd").and_then(|c| c.as_f64());
        }
    }

    cost
}

/// Get a session-cost breakdown, falling back to an estimate from per-model
/// token usage when CC's authoritative `total_cost_usd` is absent.
///
/// This is the right function to call from termination paths (process kill,
/// watchdog abort, any `stop_reason != end_turn`) where the `type:result`
/// envelope was never written. For paths that already have a guaranteed
/// clean exit, plain [`session_cost`] is sufficient.
///
/// The returned `SessionCost.total_cost_usd` is populated when either:
/// - CC wrote `total_cost_usd` into `type:result` (authoritative), OR
/// - per-model usage aggregation produced a non-zero estimate.
///
/// If the stream is empty or unparseable, `total_cost_usd` stays `None`.
pub fn session_cost_or_estimate(stream_path: &Path) -> SessionCost {
    let mut cost = session_cost(stream_path);
    if cost.total_cost_usd.is_some() {
        return cost;
    }
    let estimate = cost.estimated_cost_usd();
    if estimate > 0.0 {
        cost.total_cost_usd = Some(estimate);
    }
    cost
}

fn extract_assistant_content(val: &serde_json::Value) -> (String, Vec<ToolCall>) {
    let arr = match val.pointer("/message/content").and_then(|c| c.as_array()) {
        Some(a) => a,
        None => return (String::new(), Vec::new()),
    };

    let mut text_parts = Vec::new();
    let mut tool_calls = Vec::new();

    for block in arr {
        let block_type = block.get("type").and_then(|t| t.as_str()).unwrap_or("");
        match block_type {
            "text" => {
                if let Some(t) = block.get("text").and_then(|t| t.as_str()) {
                    text_parts.push(t.to_string());
                }
            }
            "tool_use" => {
                let name = block
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or("")
                    .to_string();
                let id = block
                    .get("id")
                    .and_then(|i| i.as_str())
                    .unwrap_or("")
                    .to_string();
                let input_summary = block
                    .get("input")
                    .map(|i| {
                        let s = i.to_string();
                        if s.len() > 100 {
                            format!("{}...", &s[..s.floor_char_boundary(100)])
                        } else {
                            s
                        }
                    })
                    .unwrap_or_default();
                tool_calls.push(ToolCall {
                    id,
                    name,
                    input_summary,
                });
            }
            _ => {}
        }
    }

    (text_parts.join("\n"), tool_calls)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_test_stream(dir: &Path, content: &str) -> std::path::PathBuf {
        std::fs::create_dir_all(dir).ok();
        let path = dir.join("test.jsonl");
        std::fs::write(&path, content).unwrap();
        path
    }

    #[test]
    fn parse_messages_basic() {
        let dir = std::env::temp_dir().join("mando-cc-transcript-test");
        let content = [
            r#"{"type":"system","subtype":"init","session_id":"abc"}"#,
            r#"{"type":"user","uuid":"u1","message":{"role":"user","content":"hello"}}"#,
            r#"{"type":"assistant","uuid":"a1","parentUuid":"u1","message":{"role":"assistant","model":"claude","content":[{"type":"text","text":"Hi there!"}],"usage":{"input_tokens":10,"output_tokens":5}}}"#,
        ].join("\n");
        let path = write_test_stream(&dir, &content);

        let msgs = parse_messages(&path, None, 0);
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].role, "user");
        assert_eq!(msgs[1].role, "assistant");
        assert_eq!(msgs[1].text, "Hi there!");
        assert_eq!(msgs[1].usage.as_ref().unwrap().input_tokens, 10);

        std::fs::remove_file(&path).ok();
        std::fs::remove_dir(&dir).ok();
    }

    #[test]
    fn tool_usage_counts() {
        let dir = std::env::temp_dir().join("mando-cc-tool-usage-test");
        let content = [
            r#"{"type":"system","subtype":"init"}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"tu1","name":"Read","input":{"path":"/tmp"}},{"type":"tool_use","id":"tu2","name":"Read","input":{"path":"/tmp/2"}}]}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"tu3","name":"Bash","input":{"command":"ls"}}]}}"#,
        ].join("\n");
        let path = write_test_stream(&dir, &content);

        let usage = tool_usage(&path);
        assert_eq!(usage.len(), 2);
        let read = usage.iter().find(|u| u.name == "Read").unwrap();
        assert_eq!(read.call_count, 2);
        let bash = usage.iter().find(|u| u.name == "Bash").unwrap();
        assert_eq!(bash.call_count, 1);

        std::fs::remove_file(&path).ok();
        std::fs::remove_dir(&dir).ok();
    }

    #[test]
    fn session_cost_aggregation() {
        let dir = std::env::temp_dir().join("mando-cc-cost-test");
        let content = [
            r#"{"type":"system","subtype":"init"}"#,
            r#"{"type":"assistant","message":{"content":[],"usage":{"input_tokens":100,"output_tokens":50,"cache_read_input_tokens":10}}}"#,
            r#"{"type":"assistant","message":{"content":[],"usage":{"input_tokens":200,"output_tokens":100}}}"#,
            r#"{"type":"result","total_cost_usd":0.05}"#,
        ].join("\n");
        let path = write_test_stream(&dir, &content);

        let cost = session_cost(&path);
        assert_eq!(cost.total_input_tokens, 300);
        assert_eq!(cost.total_output_tokens, 150);
        assert_eq!(cost.total_cache_read_tokens, 10);
        assert_eq!(cost.turn_count, 2);
        assert_eq!(cost.total_cost_usd, Some(0.05));

        std::fs::remove_file(&path).ok();
        std::fs::remove_dir(&dir).ok();
    }

    #[test]
    fn session_cost_per_model_usage() {
        // Capture model names on each assistant message so cost estimation
        // can apply per-model rates. Two turns from opus, one from sonnet.
        let dir = std::env::temp_dir().join("mando-cc-cost-per-model-test");
        let content = [
            r#"{"type":"system","subtype":"init"}"#,
            r#"{"type":"assistant","message":{"model":"claude-opus-4-7","content":[],"usage":{"input_tokens":100,"output_tokens":50}}}"#,
            r#"{"type":"assistant","message":{"model":"claude-opus-4-7","content":[],"usage":{"input_tokens":200,"output_tokens":100}}}"#,
            r#"{"type":"assistant","message":{"model":"claude-sonnet-4-6","content":[],"usage":{"input_tokens":1000,"output_tokens":500}}}"#,
        ]
        .join("\n");
        let path = write_test_stream(&dir, &content);

        let cost = session_cost(&path);
        assert_eq!(cost.per_model_usage.len(), 2);
        let opus = cost.per_model_usage.get("claude-opus-4-7").unwrap();
        assert_eq!(opus.input_tokens, 300);
        assert_eq!(opus.output_tokens, 150);
        let sonnet = cost.per_model_usage.get("claude-sonnet-4-6").unwrap();
        assert_eq!(sonnet.input_tokens, 1000);
        assert_eq!(sonnet.output_tokens, 500);

        std::fs::remove_file(&path).ok();
        std::fs::remove_dir(&dir).ok();
    }

    #[test]
    fn session_cost_or_estimate_prefers_authoritative_cost() {
        // When `type:result` carries `total_cost_usd`, the wrapper must
        // return that value verbatim and not replace it with the estimate.
        let dir = std::env::temp_dir().join("mando-cc-cost-authoritative-test");
        let content = [
            r#"{"type":"system","subtype":"init"}"#,
            r#"{"type":"assistant","message":{"model":"claude-opus-4-7","content":[],"usage":{"input_tokens":1000,"output_tokens":500}}}"#,
            r#"{"type":"result","total_cost_usd":0.1234}"#,
        ]
        .join("\n");
        let path = write_test_stream(&dir, &content);

        let cost = session_cost_or_estimate(&path);
        assert_eq!(cost.total_cost_usd, Some(0.1234));

        std::fs::remove_file(&path).ok();
        std::fs::remove_dir(&dir).ok();
    }

    #[test]
    fn session_cost_or_estimate_falls_back_on_watchdog_abort() {
        // No `type:result` message (watchdog abort, process kill, rate
        // limit). The wrapper must compute a non-zero estimate from the
        // per-model usage fields so `cc_sessions.cost_usd` stops silently
        // recording NULL on abnormal exits (task #81 worker-81-1 root cause).
        let dir = std::env::temp_dir().join("mando-cc-cost-fallback-test");
        let content = [
            r#"{"type":"system","subtype":"init"}"#,
            r#"{"type":"assistant","message":{"model":"claude-opus-4-7","content":[],"usage":{"input_tokens":1000,"output_tokens":500,"cache_read_input_tokens":50000}}}"#,
        ]
        .join("\n");
        let path = write_test_stream(&dir, &content);

        let cost = session_cost_or_estimate(&path);
        let estimate = cost.total_cost_usd.expect("estimate must be populated");
        // 1000 input * $15/Mtok + 500 output * $75/Mtok + 50k cache-read * $1.5/Mtok
        // = 0.015 + 0.0375 + 0.075 = 0.1275
        assert!(
            (estimate - 0.1275).abs() < 0.0001,
            "expected ~0.1275, got {estimate}"
        );

        std::fs::remove_file(&path).ok();
        std::fs::remove_dir(&dir).ok();
    }

    #[test]
    fn session_cost_or_estimate_empty_stream_stays_none() {
        // An empty / unparseable stream must not fabricate a non-zero cost;
        // the DB row should remain NULL rather than showing fake spend.
        let dir = std::env::temp_dir().join("mando-cc-cost-empty-test");
        let path = write_test_stream(&dir, "");

        let cost = session_cost_or_estimate(&path);
        assert_eq!(cost.total_cost_usd, None);
        assert_eq!(cost.estimated_cost_usd(), 0.0);

        std::fs::remove_file(&path).ok();
        std::fs::remove_dir(&dir).ok();
    }

    #[test]
    fn estimated_cost_usd_sums_per_model_rates() {
        // Mix of opus and haiku turns — the estimate must apply different
        // rates per model and sum the result.
        let cost = SessionCost {
            total_input_tokens: 1_100_000,
            total_output_tokens: 600_000,
            total_cache_read_tokens: 0,
            total_cache_creation_tokens: 0,
            turn_count: 2,
            total_cost_usd: None,
            per_model_usage: HashMap::from([
                (
                    "claude-opus-4-7".to_string(),
                    ModelUsage {
                        input_tokens: 100_000,
                        output_tokens: 50_000,
                        cache_read_tokens: 0,
                        cache_creation_tokens: 0,
                    },
                ),
                (
                    "claude-haiku-4-5".to_string(),
                    ModelUsage {
                        input_tokens: 1_000_000,
                        output_tokens: 550_000,
                        cache_read_tokens: 0,
                        cache_creation_tokens: 0,
                    },
                ),
            ]),
        };

        // Opus: 0.1M * $15 + 0.05M * $75 = 1.5 + 3.75 = 5.25
        // Haiku: 1M * $1 + 0.55M * $5 = 1.0 + 2.75 = 3.75
        // Total: 9.00
        let total = cost.estimated_cost_usd();
        assert!((total - 9.0).abs() < 0.0001, "got {total}");
    }

    #[test]
    fn parse_messages_pagination() {
        let dir = std::env::temp_dir().join("mando-cc-pagination-test");
        let content = [
            r#"{"type":"system","subtype":"init"}"#,
            r#"{"type":"user","uuid":"u1","message":{"role":"user","content":"q1"}}"#,
            r#"{"type":"assistant","uuid":"a1","message":{"content":[{"type":"text","text":"a1"}]}}"#,
            r#"{"type":"user","uuid":"u2","message":{"role":"user","content":"q2"}}"#,
            r#"{"type":"assistant","uuid":"a2","message":{"content":[{"type":"text","text":"a2"}]}}"#,
        ].join("\n");
        let path = write_test_stream(&dir, &content);

        let page1 = parse_messages(&path, Some(2), 0);
        assert_eq!(page1.len(), 2);
        assert_eq!(page1[0].role, "user");

        let page2 = parse_messages(&path, Some(2), 2);
        assert_eq!(page2.len(), 2);
        assert_eq!(page2[0].role, "user");

        std::fs::remove_file(&path).ok();
        std::fs::remove_dir(&dir).ok();
    }
}
