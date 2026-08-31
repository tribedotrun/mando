//! Provider-neutral canonical JSONL session-stream helpers.

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

const TAIL_READ_MAX_BYTES: u64 = 1024 * 1024;

fn is_init_event(line: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(line)
        .ok()
        .is_some_and(|value| {
            value.get("type").and_then(|kind| kind.as_str()) == Some("system")
                && value.get("subtype").and_then(|kind| kind.as_str()) == Some("init")
        })
}

fn read_tail(stream_path: &Path, max_bytes: u64) -> std::io::Result<(String, bool)> {
    let mut file = std::fs::File::open(stream_path)?;
    let len = file.metadata()?.len();
    if len <= max_bytes {
        let mut content = String::new();
        file.read_to_string(&mut content)?;
        return Ok((content, false));
    }
    let start = len - max_bytes;
    file.seek(SeekFrom::Start(start))?;
    let mut bytes = Vec::with_capacity(max_bytes as usize);
    file.take(max_bytes).read_to_end(&mut bytes)?;
    let content = String::from_utf8_lossy(&bytes).into_owned();
    let trimmed = match content.find('\n') {
        Some(newline) => content[newline + 1..].to_string(),
        None => content,
    };
    Ok((trimmed, true))
}

/// Read a stream file and locate the current session's last init event.
pub fn current_session_lines(stream_path: &Path) -> Option<(String, usize)> {
    let (content, truncated) = read_tail(stream_path, TAIL_READ_MAX_BYTES).ok()?;
    let lines: Vec<&str> = content.lines().collect();
    if let Some(index) = lines.iter().rposition(|line| is_init_event(line)) {
        return Some((content, index));
    }
    if !truncated {
        return Some((content, 0));
    }
    let content = std::fs::read_to_string(stream_path).ok()?;
    let lines: Vec<&str> = content.lines().collect();
    let last_init_index = lines
        .iter()
        .rposition(|line| is_init_event(line))
        .unwrap_or(0);
    Some((content, last_init_index))
}

/// Append a synthetic error result to a canonical session stream.
pub fn write_error_result(stream_path: &Path, error: &str) {
    let line = serde_json::json!({
        "type": "result",
        "subtype": api_types::ResultOutcome::ErrorDuringExecution.as_str(),
        "is_error": true,
        "error": error,
    });
    write_synthetic_result(stream_path, &line);
}

/// Append a synthetic interrupted result to a canonical session stream.
pub fn write_interrupted_result(stream_path: &Path) {
    let outcome = api_types::ResultOutcome::Interrupted;
    let line = serde_json::json!({
        "type": "result",
        "subtype": outcome.as_str(),
        "is_error": outcome.is_error(),
        "result": "Agent session stopped before completion",
    });
    write_synthetic_result(stream_path, &line);
}

fn write_synthetic_result(stream_path: &Path, line: &serde_json::Value) {
    use std::io::Write;
    use std::os::unix::io::AsRawFd;

    if let Some(parent) = stream_path.parent() {
        if let Err(error) = std::fs::create_dir_all(parent) {
            tracing::warn!(
                module = "agent-runtime-core",
                parent = %parent.display(),
                %error,
                "failed to pre-create stream directory before synthetic result write",
            );
        }
    }
    let mut file = match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(stream_path)
    {
        Ok(file) => file,
        Err(error) => {
            tracing::warn!(module = "agent-runtime-core", %error, "failed to write synthetic result to stream");
            return;
        }
    };
    let locked = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } == 0;
    if !locked {
        tracing::warn!(
            module = "agent-runtime-core",
            path = %stream_path.display(),
            "failed to acquire exclusive flock on stream; writing without serialization"
        );
    }
    if let Err(error) = writeln!(file, "{}", line) {
        tracing::warn!(module = "agent-runtime-core", %error, path = %stream_path.display(), "failed to write synthetic result line to stream");
    }
    if locked {
        unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_UN) };
    }
}

/// Return the newest result event from the current canonical session.
pub fn get_stream_result(stream_path: &Path) -> Option<serde_json::Value> {
    let (content, last_init_index) = current_session_lines(stream_path)?;
    let lines: Vec<&str> = content.lines().collect();
    for line in lines[last_init_index..].iter().rev() {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(line) {
            if value.get("type").and_then(|kind| kind.as_str()) == Some("result") {
                return Some(value);
            }
        }
    }
    None
}

/// Return the newest assistant text from the current canonical session.
pub fn get_last_assistant_text(stream_path: &Path) -> Option<String> {
    let (content, last_init_index) = current_session_lines(stream_path)?;
    let lines: Vec<&str> = content.lines().collect();
    for line in lines[last_init_index..].iter().rev() {
        let value: serde_json::Value = match serde_json::from_str(line) {
            Ok(value) => value,
            Err(_) => continue,
        };
        if value.get("type").and_then(|kind| kind.as_str()) != Some("assistant") {
            continue;
        }
        let content = match value
            .pointer("/message/content")
            .and_then(serde_json::Value::as_array)
        {
            Some(content) => content,
            None => continue,
        };
        for block in content.iter().rev() {
            if block.get("type").and_then(|kind| kind.as_str()) == Some("text") {
                if let Some(text) = block.get("text").and_then(|text| text.as_str()) {
                    if !text.is_empty() {
                        return Some(text.to_string());
                    }
                }
            }
        }
    }
    None
}

/// Normalize a provider result envelope into the canonical terminal outcome.
pub fn result_outcome(result: &serde_json::Value) -> api_types::ResultOutcome {
    api_types::ResultOutcome::from_subtype(
        result.get("subtype").and_then(|value| value.as_str()),
        result
            .get("is_error")
            .and_then(|value| value.as_bool())
            .unwrap_or(false),
    )
}

/// Return whether a canonical result represents successful completion.
pub fn is_clean_result(result: &serde_json::Value) -> bool {
    result_outcome(result).is_clean()
}

/// Return whether a non-empty stream has no canonical session init event.
pub fn stream_has_broken_session(stream_path: &Path) -> bool {
    let (tail, truncated) = match read_tail(stream_path, TAIL_READ_MAX_BYTES) {
        Ok(tail) if !tail.0.trim().is_empty() => tail,
        _ => return false,
    };
    if tail.lines().any(is_init_event) {
        return false;
    }
    if !truncated {
        return true;
    }
    let content = match std::fs::read_to_string(stream_path) {
        Ok(content) if !content.trim().is_empty() => content,
        _ => return false,
    };
    !content.lines().any(is_init_event)
}

/// Return seconds since the stream was last modified.
pub fn stream_stale_seconds(stream_path: &Path) -> Option<f64> {
    let metadata = std::fs::metadata(stream_path).ok()?;
    let modified = metadata.modified().ok()?;
    Some(modified.elapsed().ok()?.as_secs_f64())
}

/// Return the stream size in bytes, or zero when it is missing.
pub fn get_stream_file_size(stream_path: &Path) -> u64 {
    std::fs::metadata(stream_path)
        .map(|metadata| metadata.len())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn result_is_scoped_to_current_session() -> anyhow::Result<()> {
        let file = tempfile::NamedTempFile::new()?;
        std::fs::write(
            file.path(),
            [
                r#"{"type":"system","subtype":"init"}"#,
                r#"{"type":"result","subtype":"success","result":"old"}"#,
                r#"{"type":"system","subtype":"init"}"#,
                r#"{"type":"result","subtype":"success","result":"current"}"#,
            ]
            .join("\n"),
        )?;

        let result = get_stream_result(file.path())
            .ok_or_else(|| anyhow::anyhow!("current result missing"))?;
        assert_eq!(result["result"].as_str(), Some("current"));
        Ok(())
    }

    #[test]
    fn interrupted_result_uses_canonical_outcome() -> anyhow::Result<()> {
        let file = tempfile::NamedTempFile::new()?;
        write_interrupted_result(file.path());

        let result = get_stream_result(file.path())
            .ok_or_else(|| anyhow::anyhow!("interrupted result missing"))?;
        assert_eq!(
            result_outcome(&result),
            api_types::ResultOutcome::Interrupted
        );
        assert!(!is_clean_result(&result));
        assert_eq!(result["is_error"], serde_json::json!(false));
        Ok(())
    }
}
