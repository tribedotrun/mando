//! `mando-cc` — Internal Agent SDK for Claude Code.
//!
//! Unified crate for all `claude` CLI invocations. Two modes:
//! - [`CcSession`] — multi-turn bidirectional (stdin stays open)
//! - [`CcOneShot`] — single-turn (send prompt, wait for result, close stdin)
//!
//! Both modes use `--input-format stream-json` (never `-p`).
//! Both modes support hooks via the control protocol.
//! Both modes support `--json-schema` for structured output.

mod binary;
mod config;
pub mod hooks;
mod message;
mod oneshot;
mod process;
mod protocol;
mod session;
mod stream;
pub mod transcript;

pub use binary::resolve_claude_binary;
pub use config::{CcConfig, Effort, PermissionMode, TaskBudget, ThinkingConfig};
pub use message::{
    AssistantMessage, CcMessage, ContentBlock, InitMessage, RateLimitEvent, RateLimitStatus,
    ResultMessage, ResultSubtype,
};
pub use oneshot::CcOneShot;
pub use process::{get_cpu_time, is_process_alive, kill_process, spawn_detached};
pub use session::CcSession;
pub use stream::{
    current_session_lines, get_last_assistant_text, get_last_stream_event_type,
    get_stream_file_size, get_stream_result, is_clean_result, stream_has_broken_session,
    stream_stale_seconds, write_error_result,
};

/// Result from a CC invocation with optional structured output.
pub struct CcResult<T = serde_json::Value> {
    /// Plain text result from the model.
    pub text: String,
    /// Typed structured output (from `--json-schema`).
    pub structured: Option<T>,
    /// Session ID assigned by CC.
    pub session_id: String,
    /// Total cost in USD.
    pub cost_usd: Option<f64>,
    /// Duration in milliseconds.
    pub duration_ms: Option<u64>,
    /// API-side duration in milliseconds (excludes tool execution).
    pub duration_api_ms: Option<u64>,
    /// Number of turns executed.
    pub num_turns: Option<u32>,
    /// Error strings collected during execution (e.g. API errors, tool failures).
    pub errors: Vec<String>,
    /// Raw result envelope for advanced introspection.
    pub envelope: serde_json::Value,
    /// Path to the JSONL stream file.
    pub stream_path: std::path::PathBuf,
    /// Most recent rate limit event observed during the session (if any).
    pub rate_limit: Option<RateLimitEvent>,
    /// PID of the CC process (0 if unknown / already exited).
    pub pid: u32,
}

/// Metadata for session logging.
pub struct SessionMeta<'a> {
    pub session_id: &'a str,
    pub caller: &'a str,
    pub task_id: &'a str,
    pub worker_name: &'a str,
    pub project: &'a str,
    pub cwd: &'a str,
}

/// Write a `.meta.json` sidecar for a stream session.
pub fn write_stream_meta(meta: &SessionMeta<'_>, status: &str) {
    let meta_path = mando_config::stream_meta_path_for_session(meta.session_id);
    if let Some(parent) = meta_path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            tracing::warn!(session_id = meta.session_id, %e, "failed to create cc-streams dir");
            return;
        }
    }
    let now = mando_types::now_rfc3339();
    let val = serde_json::json!({
        "session_id": meta.session_id,
        "caller": meta.caller,
        "task_id": meta.task_id,
        "worker_name": null_if_empty(meta.worker_name),
        "project": null_if_empty(meta.project),
        "started_at": now,
        "status": status,
        "cwd": meta.cwd,
    });
    if let Err(e) = std::fs::write(
        &meta_path,
        serde_json::to_string_pretty(&val).unwrap_or_default(),
    ) {
        tracing::warn!(session_id = meta.session_id, %e, "failed to write stream meta");
    }
}

/// Update the status field in a stream meta sidecar.
pub fn update_stream_meta_status(session_id: &str, status: &str, cost_usd: Option<f64>) {
    let meta_path = mando_config::stream_meta_path_for_session(session_id);
    let data = match std::fs::read_to_string(&meta_path) {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!(session_id, %e, "failed to read stream meta for status update");
            return;
        }
    };
    let mut val: serde_json::Value = match serde_json::from_str(&data) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(session_id, %e, "corrupt stream meta sidecar");
            return;
        }
    };
    val["status"] = serde_json::json!(status);
    val["finished_at"] = serde_json::json!(mando_types::now_rfc3339());
    if let Some(cost) = cost_usd {
        val["cost_usd"] = serde_json::json!(cost);
    }
    if let Err(e) = std::fs::write(
        &meta_path,
        serde_json::to_string_pretty(&val).unwrap_or_default(),
    ) {
        tracing::warn!(session_id, %e, "failed to write updated stream meta");
    }
}

fn null_if_empty(s: &str) -> serde_json::Value {
    if s.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::json!(s)
    }
}
