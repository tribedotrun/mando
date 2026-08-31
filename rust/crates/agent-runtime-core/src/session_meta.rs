//! Canonical provider-session metadata sidecars.

use std::path::Path;

/// Provider-neutral metadata written beside a canonical session stream.
pub struct SessionMeta<'a> {
    pub session_id: &'a str,
    pub caller: &'a str,
    pub task_id: &'a str,
    pub worker_name: &'a str,
    pub project: &'a str,
    pub cwd: &'a str,
}

/// Write a running-session metadata sidecar at an adapter-selected path.
pub fn write_stream_meta_at(meta_path: &Path, meta: &SessionMeta<'_>, status: &str) {
    if let Some(parent) = meta_path.parent() {
        if let Err(error) = std::fs::create_dir_all(parent) {
            tracing::warn!(module = "agent-runtime-core", session_id = meta.session_id, path = %parent.display(), %error, "failed to create stream meta dir");
            return;
        }
    }
    let value = serde_json::json!({
        "session_id": meta.session_id,
        "caller": meta.caller,
        "task_id": meta.task_id,
        "worker_name": null_if_empty(meta.worker_name),
        "project": null_if_empty(meta.project),
        "started_at": global_infra::clock::now_rfc3339(),
        "status": status,
        "cwd": meta.cwd,
    });
    if let Err(error) = std::fs::write(
        meta_path,
        serde_json::to_string_pretty(&value).unwrap_or_default(),
    ) {
        tracing::warn!(module = "agent-runtime-core", session_id = meta.session_id, %error, "failed to write stream meta");
    }
}

/// Mark an adapter-selected metadata sidecar finished.
pub fn update_stream_meta_status_at(
    meta_path: &Path,
    session_id: &str,
    status: &str,
    cost_usd: Option<f64>,
) {
    let data = match std::fs::read_to_string(meta_path) {
        Ok(data) => data,
        Err(error) => {
            tracing::warn!(module = "agent-runtime-core", session_id, %error, "failed to read stream meta for status update");
            return;
        }
    };
    let mut value: serde_json::Value = match serde_json::from_str(&data) {
        Ok(value) => value,
        Err(error) => {
            tracing::warn!(module = "agent-runtime-core", session_id, %error, "corrupt stream meta sidecar");
            return;
        }
    };
    value["status"] = serde_json::json!(status);
    value["finished_at"] = serde_json::json!(global_infra::clock::now_rfc3339());
    if let Some(cost) = cost_usd {
        value["cost_usd"] = serde_json::json!(cost);
    }
    if let Err(error) = std::fs::write(
        meta_path,
        serde_json::to_string_pretty(&value).unwrap_or_default(),
    ) {
        tracing::warn!(module = "agent-runtime-core", session_id, %error, "failed to write updated stream meta");
    }
}

/// Return whether a metadata sidecar carries a completion timestamp.
pub fn is_stream_meta_finished_at(meta_path: &Path) -> bool {
    let data = match std::fs::read_to_string(meta_path) {
        Ok(data) => data,
        Err(_) => return false,
    };
    let value: serde_json::Value = match serde_json::from_str(&data) {
        Ok(value) => value,
        Err(_) => return false,
    };
    value
        .get("finished_at")
        .and_then(serde_json::Value::as_str)
        .is_some()
}

fn null_if_empty(value: &str) -> serde_json::Value {
    if value.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::json!(value)
    }
}
