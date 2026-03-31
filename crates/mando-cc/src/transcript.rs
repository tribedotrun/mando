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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionCost {
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cache_read_tokens: u64,
    pub total_cache_creation_tokens: u64,
    pub turn_count: u32,
    pub total_cost_usd: Option<f64>,
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
pub fn tool_usage(stream_path: &Path) -> Vec<ToolUsageSummary> {
    let (content, last_init_idx) = match crate::stream::current_session_lines(stream_path) {
        Some(c) => c,
        None => return Vec::new(),
    };

    let lines: Vec<&str> = content.lines().collect();
    let mut tools: HashMap<String, (u32, u32)> = HashMap::new();
    // Reverse lookup: tool_use_id → tool name (for attributing errors).
    let mut tool_use_id_to_name: HashMap<String, String> = HashMap::new();

    for line in &lines[last_init_idx..] {
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
pub fn session_cost(stream_path: &Path) -> SessionCost {
    let (content, last_init_idx) = match crate::stream::current_session_lines(stream_path) {
        Some(c) => c,
        None => {
            return SessionCost {
                total_input_tokens: 0,
                total_output_tokens: 0,
                total_cache_read_tokens: 0,
                total_cache_creation_tokens: 0,
                turn_count: 0,
                total_cost_usd: None,
            }
        }
    };

    let lines: Vec<&str> = content.lines().collect();
    let mut cost = SessionCost {
        total_input_tokens: 0,
        total_output_tokens: 0,
        total_cache_read_tokens: 0,
        total_cache_creation_tokens: 0,
        turn_count: 0,
        total_cost_usd: None,
    };

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
            if let Some(usage) = val.pointer("/message/usage") {
                cost.total_input_tokens += usage
                    .get("input_tokens")
                    .and_then(|t| t.as_u64())
                    .unwrap_or(0);
                cost.total_output_tokens += usage
                    .get("output_tokens")
                    .and_then(|t| t.as_u64())
                    .unwrap_or(0);
                cost.total_cache_read_tokens += usage
                    .get("cache_read_input_tokens")
                    .and_then(|t| t.as_u64())
                    .unwrap_or(0);
                cost.total_cache_creation_tokens += usage
                    .get("cache_creation_input_tokens")
                    .and_then(|t| t.as_u64())
                    .unwrap_or(0);
            }
        }

        if msg_type == "result" {
            cost.total_cost_usd = val.get("total_cost_usd").and_then(|c| c.as_f64());
        }
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
