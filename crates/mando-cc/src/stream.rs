//! Stream JSONL file introspection utilities.

use std::path::Path;

/// Read a stream file and return content + index of last session's init event.
pub fn current_session_lines(stream_path: &Path) -> Option<(String, usize)> {
    let content = std::fs::read_to_string(stream_path).ok()?;
    let lines: Vec<&str> = content.lines().collect();
    let last_init_idx = lines
        .iter()
        .rposition(|line| {
            serde_json::from_str::<serde_json::Value>(line)
                .ok()
                .map(|v| {
                    v.get("type").and_then(|t| t.as_str()) == Some("system")
                        && v.get("subtype").and_then(|s| s.as_str()) == Some("init")
                })
                .unwrap_or(false)
        })
        .unwrap_or(0);
    Some((content, last_init_idx))
}

/// Get the result event from the current session in a JSONL stream log.
pub fn get_stream_result(stream_path: &Path) -> Option<serde_json::Value> {
    let (content, last_init_idx) = current_session_lines(stream_path)?;
    let lines: Vec<&str> = content.lines().collect();
    for line in lines[last_init_idx..].iter().rev() {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(line) {
            if val.get("type").and_then(|t| t.as_str()) == Some("result") {
                return Some(val);
            }
        }
    }
    None
}

/// Get last assistant text content from the current session.
pub fn get_last_assistant_text(stream_path: &Path) -> Option<String> {
    let (content, last_init_idx) = current_session_lines(stream_path)?;
    let lines: Vec<&str> = content.lines().collect();
    for line in lines[last_init_idx..].iter().rev() {
        let val: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if val.get("type").and_then(|t| t.as_str()) != Some("assistant") {
            continue;
        }
        let arr = match val.pointer("/message/content").and_then(|c| c.as_array()) {
            Some(a) => a,
            None => continue,
        };
        for block in arr.iter().rev() {
            if block.get("type").and_then(|t| t.as_str()) == Some("text") {
                if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                    if !text.is_empty() {
                        return Some(text.to_string());
                    }
                }
            }
        }
    }
    None
}

/// Check if a stream result indicates clean completion.
pub fn is_clean_result(result: &serde_json::Value) -> bool {
    if let Some(subtype) = result.get("subtype").and_then(|s| s.as_str()) {
        return subtype == "success";
    }
    if let Some(is_error) = result.get("is_error").and_then(|e| e.as_bool()) {
        return !is_error;
    }
    false
}

/// Check if a stream file has a broken session (content but zero init events).
pub fn stream_has_broken_session(stream_path: &Path) -> bool {
    let content = match std::fs::read_to_string(stream_path) {
        Ok(c) if !c.trim().is_empty() => c,
        _ => return false,
    };
    !content.lines().any(|line| {
        serde_json::from_str::<serde_json::Value>(line)
            .ok()
            .map(|v| {
                v.get("type").and_then(|t| t.as_str()) == Some("system")
                    && v.get("subtype").and_then(|s| s.as_str()) == Some("init")
            })
            .unwrap_or(false)
    })
}

/// Get the last stream event type from the current session.
pub fn get_last_stream_event_type(stream_path: &Path) -> Option<String> {
    let (content, last_init_idx) = current_session_lines(stream_path)?;
    let lines: Vec<&str> = content.lines().collect();
    for line in lines[last_init_idx..].iter().rev() {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(line) {
            if let Some(t) = val.get("type").and_then(|t| t.as_str()) {
                return Some(t.to_string());
            }
        }
    }
    None
}

/// Seconds since last stream file modification.
pub fn stream_stale_seconds(stream_path: &Path) -> Option<f64> {
    let metadata = std::fs::metadata(stream_path).ok()?;
    let modified = metadata.modified().ok()?;
    Some(modified.elapsed().ok()?.as_secs_f64())
}

/// Get the size in bytes of a stream file (0 if missing).
pub fn get_stream_file_size(stream_path: &Path) -> u64 {
    std::fs::metadata(stream_path).map(|m| m.len()).unwrap_or(0)
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
        assert_eq!(get_stream_file_size(Path::new("/nonexistent")), 0);
    }
}
