//! Spawn/kill/resume CC subprocess management.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Result;

/// Resolve the `claude` binary path.
///
/// Checks `MANDO_CC_CLAUDE_BIN` first (integration tests use `mando-cc-mock`),
/// then PATH, then known install locations.
pub fn resolve_claude_binary() -> PathBuf {
    // 0. MANDO_CC_CLAUDE_BIN override (integration tests).
    if let Ok(p) = std::env::var("MANDO_CC_CLAUDE_BIN") {
        let pb = PathBuf::from(&p);
        if !pb.as_os_str().is_empty() && (pb.is_absolute() || pb.exists()) {
            return pb;
        }
    }

    // 1. Check PATH via which.
    if let Ok(output) = std::process::Command::new("which").arg("claude").output() {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return PathBuf::from(path);
            }
        }
    }

    // 2. Check known locations.
    let home = std::env::var("HOME").unwrap_or_default();
    let candidates = [
        format!("{}/.npm-global/bin/claude", home),
        format!("{}/.local/bin/claude", home),
        "/usr/local/bin/claude".to_string(),
    ];
    for c in &candidates {
        if Path::new(c).exists() {
            return PathBuf::from(c);
        }
    }

    PathBuf::from("claude")
}

/// Spawn a long-lived worker CC process with stream-json output.
///
/// Delegates to `mando_cc::spawn_detached`. Output is written to
/// `cc-streams/{session_id}.jsonl`. Returns `(pid, stdout_path)`.
pub(crate) async fn spawn_worker_process(
    _session_name: &str,
    prompt: &str,
    cwd: &Path,
    model: &str,
    session_id: &str,
    env_overrides: &HashMap<String, String>,
    fallback_model: Option<&str>,
) -> Result<(u32, PathBuf)> {
    let mut builder = mando_cc::CcConfig::builder()
        .model(model)
        .effort(mando_cc::Effort::Max)
        .cwd(cwd)
        .session_id(session_id);
    if let Some(fb) = fallback_model {
        builder = builder.fallback_model(fb);
    }
    for (k, v) in env_overrides {
        builder = builder.env(k, v);
    }
    mando_cc::spawn_detached(&builder.build(), prompt, session_id).await
}

/// Spawn a worker with --resume instead of --session-id.
///
/// Delegates to `mando_cc::spawn_detached` with resume config.
/// Appends to the existing `cc-streams/{resume_session_id}.jsonl`.
pub async fn resume_worker_process(
    _session_name: &str,
    message: &str,
    cwd: &Path,
    model: &str,
    resume_session_id: &str,
    env_overrides: &HashMap<String, String>,
    fallback_model: Option<&str>,
) -> Result<(u32, PathBuf)> {
    let mut builder = mando_cc::CcConfig::builder()
        .model(model)
        .effort(mando_cc::Effort::Max)
        .cwd(cwd)
        .resume(resume_session_id);
    if let Some(fb) = fallback_model {
        builder = builder.fallback_model(fb);
    }
    for (k, v) in env_overrides {
        builder = builder.env(k, v);
    }
    mando_cc::spawn_detached(&builder.build(), message, resume_session_id).await
}

/// Kill a worker process — delegates to `mando_cc::kill_process`.
pub async fn kill_worker_process(pid: u32) -> Result<()> {
    mando_cc::kill_process(pid).await
}

/// Check if a process is alive — delegates to `mando_cc::is_process_alive`.
pub fn is_process_alive(pid: u32) -> bool {
    mando_cc::is_process_alive(pid)
}

/// Get CPU time — delegates to `mando_cc::get_cpu_time`.
pub async fn get_cpu_time(pid: u32) -> Result<f64> {
    mando_cc::get_cpu_time(pid).await
}

// ── Stream introspection — delegates to mando_cc ────────────────────────────

/// Get the size in bytes of a stream file (0 if missing).
pub fn get_stream_file_size(stream_path: &Path) -> u64 {
    mando_cc::get_stream_file_size(stream_path)
}

/// Get the result from the current session in a JSONL stream log.
pub fn get_stream_result(stream_path: &Path) -> Option<serde_json::Value> {
    mando_cc::get_stream_result(stream_path)
}

/// Extract last assistant text from current session.
pub fn get_last_assistant_text(stream_path: &Path) -> Option<String> {
    mando_cc::get_last_assistant_text(stream_path)
}

/// Get last stream event type.
pub fn get_last_stream_event_type(stream_path: &Path) -> Option<String> {
    mando_cc::get_last_stream_event_type(stream_path)
}

/// Check if stream result indicates clean completion.
pub fn is_clean_result(result: &serde_json::Value) -> bool {
    mando_cc::is_clean_result(result)
}

/// Check if stream has a broken session (no init events).
pub fn stream_has_broken_session(stream_path: &Path) -> bool {
    mando_cc::stream_has_broken_session(stream_path)
}

/// Seconds since last stream file modification.
pub fn stream_stale_seconds(stream_path: &Path) -> Option<f64> {
    mando_cc::stream_stale_seconds(stream_path)
}

/// Write meta sidecar — delegates to mando_cc.
pub fn write_stream_meta(
    session_id: &str,
    caller: &str,
    task_id: &str,
    worker_name: &str,
    project: &str,
    cwd: &str,
    status: &str,
) {
    mando_cc::write_stream_meta(
        &mando_cc::SessionMeta {
            session_id,
            caller,
            task_id,
            worker_name,
            project,
            cwd,
        },
        status,
    );
}

/// Update meta sidecar status — delegates to mando_cc.
pub fn update_stream_meta_status(session_id: &str, status: &str, cost_usd: Option<f64>) {
    mando_cc::update_stream_meta_status(session_id, status, cost_usd);
}

// Tests for stream functions now live in mando-cc.
// Tests for process functions (cputime, resolve_binary) also in mando-cc.
// This module is a thin delegation layer — keeping only an integration test.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_claude_returns_path() {
        let path = resolve_claude_binary();
        assert!(!path.as_os_str().is_empty());
    }

    #[test]
    fn delegation_stream_result() {
        // Verify the delegation works through the full path.
        let dir = std::env::temp_dir().join("mando-pm-delegate");
        std::fs::create_dir_all(&dir).ok();
        let path = dir.join("test.jsonl");

        let content = [
            r#"{"type":"system","subtype":"init"}"#,
            r#"{"type":"result","subtype":"success","result":"ok"}"#,
        ]
        .join("\n");
        std::fs::write(&path, &content).unwrap();

        let result = get_stream_result(&path).unwrap();
        assert!(is_clean_result(&result));
        assert_eq!(get_stream_file_size(&path), content.len() as u64);

        std::fs::remove_file(&path).ok();
        std::fs::remove_dir(&dir).ok();
    }
}
