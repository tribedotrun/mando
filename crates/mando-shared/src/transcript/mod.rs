//! Convert Claude Code session JSONL transcript to readable Markdown.
//!
//! Lossless, deterministic.

mod tool_render;

use serde_json::Value;
use tool_render::{detect_path_prefix, render_tool_use};

/// Message types to skip entirely.
const SKIP_TYPES: &[&str] = &[
    "progress",
    "file-history-snapshot",
    "summary",
    "context-pruned",
    "context-window-full",
];

/// Convert raw JSONL content into readable markdown.
pub fn jsonl_to_markdown(jsonl_content: &str) -> String {
    let messages: Vec<Value> = jsonl_content
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|msg: &Value| {
            let t = msg_str(msg, "type");
            !SKIP_TYPES.contains(&t)
        })
        .collect();

    let path_prefix = detect_path_prefix(&messages);
    let mut parts: Vec<String> = Vec::new();
    let mut human_num = 0u32;
    let mut turn_num = 0u32;
    let mut i = 0;
    let mut last_enqueue = String::new();

    while i < messages.len() {
        let msg = &messages[i];
        let msg_type = msg_str(msg, "type");

        // Queue operation
        if msg_type == "queue-operation" {
            let op = msg_str(msg, "operation");
            let ts = msg_str(msg, "timestamp");
            parts.push(format!("---\n## [{op}] {ts}\n"));
            let content = msg_str(msg, "content");
            if !content.is_empty() {
                parts.push(format!("```\n{content}\n```\n"));
                if op == "enqueue" {
                    last_enqueue = content.to_owned();
                }
            }
            i += 1;
            continue;
        }

        // Last-prompt marker
        if msg_type == "last-prompt" {
            let sid = msg.get("sessionId").and_then(Value::as_str).unwrap_or("?");
            parts.push(format!("\n---\n*Session end: {sid}*\n"));
            i += 1;
            continue;
        }

        // Real human message
        if (msg_type == "human" || msg_type == "user") && !is_tool_result_msg(msg) {
            let inner = msg.get("message").unwrap_or(msg);
            let text = extract_text_from_content(inner.get("content"));
            // Deduplicate vs enqueue
            if !last_enqueue.is_empty() && text.trim() == last_enqueue.trim() {
                last_enqueue.clear();
                i += 1;
                continue;
            }
            last_enqueue.clear();
            human_num += 1;
            let ts = msg_str(msg, "timestamp");
            let ts_part = if ts.is_empty() {
                String::new()
            } else {
                format!("  `{ts}`")
            };
            parts.push(format!("---\n## Human #{human_num}{ts_part}\n"));
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                parts.push(format!("{trimmed}\n"));
            }
            i += 1;
            continue;
        }

        // Orphan tool result — skip
        if (msg_type == "human" || msg_type == "user") && is_tool_result_msg(msg) {
            i += 1;
            continue;
        }

        // Assistant turn
        if msg_type == "assistant" {
            turn_num += 1;
            let (turn_parts, tool_results, next_i) = consume_turn(&messages, i, &path_prefix);
            i = next_i;

            let ts = msg_str(msg, "timestamp");
            let inner = msg.get("message").unwrap_or(msg);
            let model = inner.get("model").and_then(Value::as_str).unwrap_or("");
            let mut header = format!("---\n## Turn #{turn_num}");
            if !model.is_empty() {
                header.push_str(&format!("  `{model}`"));
            }
            if !ts.is_empty() {
                header.push_str(&format!("  `{ts}`"));
            }
            parts.push(format!("{header}\n"));
            parts.push(turn_parts.join("\n") + "\n");

            // Tool results summary
            if !tool_results.is_empty() {
                let ok = tool_results.iter().filter(|r| !r.is_error).count();
                let fail = tool_results.len() - ok;
                let summary = match (ok, fail) {
                    (0, f) => format!("{f} failed"),
                    (o, 0) => format!("{o} ok"),
                    (o, f) => format!("{o} ok, {f} failed"),
                };

                parts.push(format!("\n*results: {summary}*\n"));
                let mut shown_initial = false;
                for r in &tool_results {
                    if r.is_error {
                        parts.push(format!("**Error:**\n```\n{}\n```\n", r.text));
                    } else if turn_num == 1 && !shown_initial && !r.text.trim().is_empty() {
                        shown_initial = true;
                        parts.push(format!("**Initial context:**\n```\n{}\n```\n", r.text));
                    }
                }
            }
            continue;
        }

        // Unknown type
        i += 1;
    }

    parts.join("\n")
}

struct ToolResult {
    text: String,
    is_error: bool,
}

fn consume_turn(
    messages: &[Value],
    start: usize,
    path_prefix: &str,
) -> (Vec<String>, Vec<ToolResult>, usize) {
    let mut turn_parts: Vec<String> = Vec::new();
    let mut tool_results: Vec<ToolResult> = Vec::new();
    let mut i = start;

    while i < messages.len() {
        let msg = &messages[i];
        let msg_type = msg_str(msg, "type");

        if msg_type == "assistant" {
            let inner = msg.get("message").unwrap_or(msg);
            if let Some(content) = inner.get("content") {
                if let Some(blocks) = content.as_array() {
                    for block in blocks {
                        let btype = block.get("type").and_then(|v| v.as_str()).unwrap_or("");
                        match btype {
                            "text" => {
                                let text = block
                                    .get("text")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("")
                                    .trim();
                                if !text.is_empty() {
                                    turn_parts.push(format!("{text}\n"));
                                }
                            }
                            "tool_use" => {
                                let name =
                                    block.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                                let input = block.get("input").cloned().unwrap_or(Value::Null);
                                turn_parts.push(render_tool_use(name, &input, path_prefix) + "\n");
                            }
                            _ => {}
                        }
                    }
                } else if let Some(text) = content.as_str() {
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        turn_parts.push(format!("{trimmed}\n"));
                    }
                }
            }
            i += 1;
            continue;
        }

        // Tool result messages
        if (msg_type == "human" || msg_type == "user") && is_tool_result_msg(msg) {
            let inner = msg.get("message").unwrap_or(msg);
            if let Some(blocks) = inner.get("content").and_then(|c| c.as_array()) {
                for block in blocks {
                    if block.get("type").and_then(|v| v.as_str()) == Some("tool_result") {
                        let is_err = block
                            .get("is_error")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);
                        let text = extract_result_text(block.get("content"));
                        tool_results.push(ToolResult {
                            text,
                            is_error: is_err,
                        });
                    }
                }
            }
            i += 1;
            continue;
        }

        // End of turn
        break;
    }

    (turn_parts, tool_results, i)
}

// ── Helpers ──

fn msg_str<'a>(msg: &'a Value, key: &str) -> &'a str {
    msg.get(key).and_then(|v| v.as_str()).unwrap_or("")
}

fn is_tool_result_msg(msg: &Value) -> bool {
    let inner = msg.get("message").unwrap_or(msg);
    let content = match inner.get("content").and_then(|c| c.as_array()) {
        Some(arr) => arr,
        None => return false,
    };
    if content.is_empty() {
        return false;
    }
    content
        .iter()
        .all(|b| b.get("type").and_then(|v| v.as_str()) == Some("tool_result"))
}

fn extract_text_from_content(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(arr)) => arr
            .iter()
            .filter_map(|b| {
                if b.get("type").and_then(|v| v.as_str()) == Some("text") {
                    b.get("text").and_then(|v| v.as_str()).map(String::from)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Some(v) => v.to_string(),
        None => String::new(),
    }
}

fn extract_result_text(content: Option<&Value>) -> String {
    match content {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(arr)) => arr
            .iter()
            .map(|item| {
                if let Some(obj) = item.as_object() {
                    match obj.get("type").and_then(|v| v.as_str()) {
                        Some("text") => obj
                            .get("text")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        Some("image") => "[image]".to_string(),
                        _ => {
                            let s = serde_json::to_string(item).unwrap_or_default();
                            tool_render::truncate_str(&s, 300)
                        }
                    }
                } else {
                    item.to_string()
                }
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Some(v) => v.to_string(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_input() {
        assert_eq!(jsonl_to_markdown(""), "");
    }

    #[test]
    fn skips_progress_messages() {
        let input = r#"{"type":"progress","message":"working..."}"#;
        assert_eq!(jsonl_to_markdown(input), "");
    }

    #[test]
    fn renders_human_message() {
        let input = r#"{"type":"human","message":{"role":"user","content":"hello"}}"#;
        let md = jsonl_to_markdown(input);
        assert!(md.contains("Human #1"));
        assert!(md.contains("hello"));
    }

    #[test]
    fn renders_assistant_text() {
        let input = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"response here"}]}}"#;
        let md = jsonl_to_markdown(input);
        assert!(md.contains("Turn #1"));
        assert!(md.contains("response here"));
    }

    #[test]
    fn renders_bash_tool_use() {
        let input = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","name":"Bash","input":{"command":"ls -la","description":"list files"}}]}}"#;
        let md = jsonl_to_markdown(input);
        assert!(md.contains("**Bash**"));
        assert!(md.contains("ls -la"));
        assert!(md.contains("list files"));
    }

    #[test]
    fn renders_edit_as_diff() {
        let input = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","name":"Edit","input":{"file_path":"/foo/bar.rs","old_string":"old","new_string":"new"}}]}}"#;
        let md = jsonl_to_markdown(input);
        assert!(md.contains("**Edit**"));
        assert!(md.contains("- old"));
        assert!(md.contains("+ new"));
    }
}
