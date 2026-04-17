mod binary;
pub mod codex_exec;
mod config;
pub mod credentials;
mod error;
pub mod json_parse;
mod message;
mod oneshot;
mod process;
mod protocol;
mod session;
mod stream;
pub mod transcript;

pub use binary::resolve_claude_binary;
pub use config::{CcConfig, CcConfigBuilder, Effort, PermissionMode, TaskBudget, ThinkingConfig};
pub use error::CcError;
pub use message::{
    AssistantMessage, CcMessage, ContentBlock, InitMessage, RateLimitEvent, RateLimitStatus,
    ResultMessage, ResultSubtype,
};
pub use oneshot::CcOneShot;
pub use process::{get_cpu_time, is_process_alive, kill_process, spawn_detached};
pub use session::CcSession;
pub use stream::{
    get_last_assistant_text, get_stream_cost, get_stream_file_size, get_stream_result,
    has_rate_limit_rejection, is_clean_result, last_rate_limit_status, stream_has_broken_session,
    stream_stale_seconds, write_error_result, RateLimitRejection, StreamCostInfo,
    StreamRateLimitInfo,
};

pub struct CcResult<T = serde_json::Value> {
    pub text: String,
    pub structured: Option<T>,
    pub session_id: String,
    pub cost_usd: Option<f64>,
    pub duration_ms: Option<u64>,
    pub duration_api_ms: Option<u64>,
    pub num_turns: Option<u32>,
    pub errors: Vec<String>,
    pub envelope: serde_json::Value,
    pub stream_path: std::path::PathBuf,
    pub rate_limit: Option<RateLimitEvent>,
    pub pid: global_types::Pid,
}

pub struct SessionMeta<'a> {
    pub session_id: &'a str,
    pub caller: &'a str,
    pub task_id: &'a str,
    pub worker_name: &'a str,
    pub project: &'a str,
    pub cwd: &'a str,
}

pub fn write_stream_meta(meta: &SessionMeta<'_>, status: &str) {
    let meta_path = global_infra::paths::stream_meta_path_for_session(meta.session_id);
    if let Some(parent) = meta_path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            tracing::warn!(session_id = meta.session_id, %e, "failed to create cc-streams dir");
            return;
        }
    }
    let now = global_infra::clock::now_rfc3339();
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

pub fn update_stream_meta_status(session_id: &str, status: &str, cost_usd: Option<f64>) {
    let meta_path = global_infra::paths::stream_meta_path_for_session(session_id);
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
    val["finished_at"] = serde_json::json!(global_infra::clock::now_rfc3339());
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

pub fn is_session_finished(session_id: &str) -> bool {
    let meta_path = global_infra::paths::stream_meta_path_for_session(session_id);
    let data = match std::fs::read_to_string(&meta_path) {
        Ok(d) => d,
        Err(_) => return false,
    };
    let val: serde_json::Value = match serde_json::from_str(&data) {
        Ok(v) => v,
        Err(_) => return false,
    };
    val.get("finished_at").and_then(|v| v.as_str()).is_some()
}

fn null_if_empty(s: &str) -> serde_json::Value {
    if s.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::json!(s)
    }
}
