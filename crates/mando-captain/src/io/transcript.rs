//! JSONL transcript read/write.

use std::path::Path;

/// Read the last N lines from a JSONL file.
pub(crate) fn read_tail(path: &Path, n: usize) -> Vec<String> {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let lines: Vec<&str> = content.lines().collect();
    let start = if lines.len() > n { lines.len() - n } else { 0 };
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

/// Append a JSONL line to a file.
#[cfg(test)]
pub(crate) fn append_jsonl(path: &Path, value: &serde_json::Value) -> anyhow::Result<()> {
    use std::io::Write;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    let line = serde_json::to_string(value)?;
    writeln!(file, "{}", line)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_tail_nonexistent() {
        let lines = read_tail(Path::new("/nonexistent.jsonl"), 10);
        assert!(lines.is_empty());
    }

    #[test]
    fn append_and_read() {
        let tmp = std::env::temp_dir().join("mando-test-transcript.jsonl");
        let _ = std::fs::remove_file(&tmp);

        append_jsonl(&tmp, &serde_json::json!({"type": "test", "n": 1})).unwrap();
        append_jsonl(&tmp, &serde_json::json!({"type": "test", "n": 2})).unwrap();

        let lines = read_tail(&tmp, 10);
        assert_eq!(lines.len(), 2);

        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn extract_stream_tail_includes_tool_use() {
        let tmp = std::env::temp_dir().join("mando-test-stream-tail.jsonl");
        let _ = std::fs::remove_file(&tmp);

        // Assistant with tool_use (no text)
        append_jsonl(
            &tmp,
            &serde_json::json!({
                "type": "assistant",
                "message": {"content": [{"type": "tool_use", "name": "Read"}]}
            }),
        )
        .unwrap();
        // tool_result
        append_jsonl(&tmp, &serde_json::json!({"type": "user"})).unwrap();

        let tail = extract_stream_tail(&tmp, 50);
        assert!(
            tail.contains("[tool_use: Read]"),
            "should include tool_use marker"
        );
        assert!(tail.contains("[user]"), "should include user marker");

        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn extract_stream_tail_respects_session_boundary() {
        let tmp = std::env::temp_dir().join("mando-test-session-boundary.jsonl");
        let _ = std::fs::remove_file(&tmp);

        // Old session: result event
        append_jsonl(
            &tmp,
            &serde_json::json!({"type": "result", "subtype": "success"}),
        )
        .unwrap();
        // New session starts (nudge-resume appends init)
        append_jsonl(
            &tmp,
            &serde_json::json!({"type": "system", "subtype": "init"}),
        )
        .unwrap();
        // New session: assistant working
        append_jsonl(
            &tmp,
            &serde_json::json!({
                "type": "assistant",
                "message": {"content": [{"type": "tool_use", "name": "Bash"}]}
            }),
        )
        .unwrap();

        let tail = extract_stream_tail(&tmp, 50);
        assert!(
            !tail.contains("[result:"),
            "old session result should be excluded: {tail}"
        );
        assert!(
            tail.contains("[tool_use: Bash]"),
            "new session events should be included"
        );

        std::fs::remove_file(&tmp).ok();
    }
}
