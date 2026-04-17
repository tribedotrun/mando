//! Stream JSONL file introspection utilities.

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// Upper bound for the tail-read window.
///
/// Stream log files grow unbounded for long-running workers, but all callers
/// of [`current_session_lines`] only need the lines from the last `system/init`
/// event onward. For sessions where the last init is within 1 MiB of EOF (the
/// overwhelmingly common case) we read only that window; otherwise we fall
/// back to a full read.
const TAIL_READ_MAX_BYTES: u64 = 1024 * 1024;

/// Check if a JSONL line is a session init event (`{"type":"system","subtype":"init"}`).
fn is_init_event(line: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(line)
        .ok()
        .is_some_and(|v| {
            v.get("type").and_then(|t| t.as_str()) == Some("system")
                && v.get("subtype").and_then(|s| s.as_str()) == Some("init")
        })
}

/// Read the last up-to-`max_bytes` of a file.
///
/// Returns `(content, truncated)` where `truncated` is true if the file is
/// longer than `max_bytes` and we only read the tail. A leading partial line
/// (everything before the first `\n` within the window) is discarded when
/// truncated, so the returned content always starts at a line boundary.
fn read_tail(stream_path: &Path, max_bytes: u64) -> std::io::Result<(String, bool)> {
    let mut file = std::fs::File::open(stream_path)?;
    let len = file.metadata()?.len();
    if len <= max_bytes {
        let mut buf = String::new();
        file.read_to_string(&mut buf)?;
        return Ok((buf, false));
    }
    let start = len - max_bytes;
    file.seek(SeekFrom::Start(start))?;
    let mut bytes = Vec::with_capacity(max_bytes as usize);
    file.take(max_bytes).read_to_end(&mut bytes)?;
    let content = String::from_utf8_lossy(&bytes).into_owned();
    // Drop any leading partial line so we always start on a full line boundary.
    let trimmed = match content.find('\n') {
        Some(nl) => content[nl + 1..].to_string(),
        None => content,
    };
    Ok((trimmed, true))
}

/// Read a stream file and return content + index of last session's init event.
///
/// For files larger than [`TAIL_READ_MAX_BYTES`] we first try a tail read of
/// the final window. If no init event is present in the tail (very long
/// current session), we fall back to a full read so correctness is preserved.
pub fn current_session_lines(stream_path: &Path) -> Option<(String, usize)> {
    let (content, truncated) = read_tail(stream_path, TAIL_READ_MAX_BYTES).ok()?;
    let lines: Vec<&str> = content.lines().collect();
    if let Some(idx) = lines.iter().rposition(|line| is_init_event(line)) {
        return Some((content, idx));
    }
    if !truncated {
        return Some((content, 0));
    }
    // Tail window contained no init event — fall back to full read.
    let content = std::fs::read_to_string(stream_path).ok()?;
    let lines: Vec<&str> = content.lines().collect();
    let last_init_idx = lines
        .iter()
        .rposition(|line| is_init_event(line))
        .unwrap_or(0);
    Some((content, last_init_idx))
}

/// Write a synthetic error result to a stream file so `get_stream_result` picks it up.
///
/// Used when an async CC task crashes before the CC process writes its own result event.
///
/// The append is serialized with an exclusive BSD `flock(2)` on the stream
/// file itself. Two concurrent writers (e.g. resumed sessions sharing the
/// same stream) would otherwise interleave JSON lines and corrupt the JSONL
/// transcript. The lock is released when `file` goes out of scope.
pub fn write_error_result(stream_path: &Path, error: &str) {
    use std::io::Write;
    use std::os::unix::io::AsRawFd;
    let line = serde_json::json!({
        "type": "result",
        "subtype": "error",
        "is_error": true,
        "error": error,
    });
    if let Some(parent) = stream_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut file = match std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(stream_path)
    {
        Ok(f) => f,
        Err(e) => {
            tracing::warn!(%e, "failed to write error result to stream");
            return;
        }
    };
    // SAFETY: `file` owns a valid fd for the body of this function.
    // `flock(LOCK_EX)` blocks until the exclusive lock is acquired; failure
    // is non-fatal — we still attempt the write (best-effort) and log a warning.
    let locked = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } == 0;
    if !locked {
        tracing::warn!(
            path = %stream_path.display(),
            "failed to acquire exclusive flock on stream — writing without serialization"
        );
    }
    if let Err(e) = writeln!(file, "{}", line) {
        tracing::warn!(%e, path = %stream_path.display(), "failed to write error result line to stream");
    }
    if locked {
        // Explicit unlock; the fd is closed on drop of `file` which also
        // releases the lock, but unlocking here makes ordering unambiguous.
        // SAFETY: fd is still valid until `file` is dropped.
        unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_UN) };
    }
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

/// Check if a stream file has a broken session (content but zero init events).
pub fn stream_has_broken_session(stream_path: &Path) -> bool {
    // Try tail first; if tail has an init, we're not broken. If tail has no
    // init but is truncated, fall back to full read so we don't false-positive.
    let (tail, truncated) = match read_tail(stream_path, TAIL_READ_MAX_BYTES) {
        Ok(t) if !t.0.trim().is_empty() => t,
        _ => return false,
    };
    if tail.lines().any(is_init_event) {
        return false;
    }
    if !truncated {
        return true;
    }
    let content = match std::fs::read_to_string(stream_path) {
        Ok(c) if !c.trim().is_empty() => c,
        _ => return false,
    };
    !content.lines().any(is_init_event)
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
