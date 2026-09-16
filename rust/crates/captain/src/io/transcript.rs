//! JSONL transcript read/write.

use std::path::Path;

/// Read the last N lines from a JSONL file.
pub(crate) fn read_tail(path: &Path, n: usize) -> Vec<String> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Vec::new(),
        Err(e) => {
            tracing::warn!(module = "captain-io-transcript", path = %path.display(), error = %e, "failed to read transcript");
            return Vec::new();
        }
    };
    let lines: Vec<&str> = content.lines().collect();
    let start = lines.len().saturating_sub(n);
    lines[start..].iter().map(|l| l.to_string()).collect()
}

/// Extract recent output text from a JSONL stream log.
///
/// Reads the last N lines and extracts text from ALL event types:
/// assistant text content, tool_use tool names, and tool_result output.
pub(crate) fn extract_stream_tail(stream_path: &Path, max_lines: usize) -> String {
    let tail = read_tail(stream_path, max_lines);

    // Respect session boundaries: only process events after the last
    // system/init event (session delimiter). Without this, result markers
    // from a previous sub-session leak in after nudge-resume.
    let start_idx = tail
        .iter()
        .rposition(|line| {
            serde_json::from_str::<serde_json::Value>(line)
                .ok()
                .is_some_and(|v| {
                    v.get("type").and_then(|t| t.as_str()) == Some("system")
                        && v.get("subtype").and_then(|s| s.as_str()) == Some("init")
                })
        })
        .map(|i| i + 1) // skip the init event itself
        .unwrap_or(0);
    let session_tail = &tail[start_idx..];

    let mut output_lines = Vec::new();

    for line in session_tail {
        let val = match serde_json::from_str::<serde_json::Value>(line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let event_type = val.get("type").and_then(|t| t.as_str()).unwrap_or("");

        match event_type {
            "assistant" => {
                if let Some(content) = val
                    .get("message")
                    .and_then(|m| m.get("content"))
                    .and_then(|c| c.as_array())
                {
                    for block in content {
                        let block_type = block.get("type").and_then(|t| t.as_str());
                        match block_type {
                            Some("text") => {
                                if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                                    for l in text.lines().rev().take(5) {
                                        output_lines.push(l.to_string());
                                    }
                                }
                            }
                            Some("tool_use") => {
                                // Include tool name so hash changes during tool-use sequences.
                                if let Some(name) = block.get("name").and_then(|n| n.as_str()) {
                                    output_lines.push(format!("[tool_use: {}]", name));
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            "user" | "tool_result" => {
                // tool_result events: include a marker so the hash changes.
                output_lines.push(format!("[{}]", event_type));
            }
            "result" => {
                let subtype = val
                    .get("subtype")
                    .and_then(|s| s.as_str())
                    .unwrap_or("unknown");
                output_lines.push(format!("[result: {}]", subtype));
            }
            _ => {}
        }
    }

    output_lines.reverse();
    output_lines.join("\n")
}
