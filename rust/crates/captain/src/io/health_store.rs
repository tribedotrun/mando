//! Worker health state read/write — `~/.mando/state/worker-health.json`.

use std::collections::HashMap;
use std::path::Path;

use anyhow::{Context, Result};
use global_infra::save_json_file;

/// Worker health entry.
pub type HealthState = HashMap<String, serde_json::Value>;

/// Load worker health state from disk.
///
/// A missing file is treated as "first boot" and returns an empty state.
/// Corrupt JSON is rotated to `.corrupt.bak` and reported as an error.
/// Any other I/O failure (permission denied, etc.) is propagated to the
/// caller so crash detection is never silently blinded.
pub fn load_health_state(path: &Path) -> Result<HealthState> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(HealthState::new()),
        Err(e) => {
            return Err(e)
                .with_context(|| format!("failed to read health state at {}", path.display()));
        }
    };

    match serde_json::from_str::<HealthState>(&text) {
        Ok(state) => Ok(state),
        Err(e) => {
            tracing::error!(
                module = "health_store",
                path = %path.display(),
                error = %e,
                "health state corrupt — rotating to .corrupt.bak"
            );
            // Preserve corrupt file for debugging.
            let bak = path.with_extension("corrupt.bak");
            if let Err(re) = std::fs::rename(path, &bak) {
                tracing::error!(
                    module = "health_store",
                    path = %path.display(),
                    backup = %bak.display(),
                    error = %re,
                    "failed to rename corrupt health state file"
                );
            }
            Err(anyhow::anyhow!(
                "health state at {} is corrupt: {e}",
                path.display()
            ))
        }
    }
}

/// Async variant of [`load_health_state`] for callers on the tokio runtime.
///
/// Mirrors the sync version but uses `tokio::fs` so async code paths (like
/// the SSE snapshot builder) don't stall the executor on a blocking read.
/// Corrupt-file recovery uses `tokio::fs::rename`.
pub async fn load_health_state_async(path: &Path) -> Result<HealthState> {
    let text = match tokio::fs::read_to_string(path).await {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(HealthState::new()),
        Err(e) => {
            return Err(e)
                .with_context(|| format!("failed to read health state at {}", path.display()));
        }
    };

    match serde_json::from_str::<HealthState>(&text) {
        Ok(state) => Ok(state),
        Err(e) => {
            tracing::error!(
                module = "health_store",
                path = %path.display(),
                error = %e,
                "health state corrupt, rotating to .corrupt.bak"
            );
            let bak = path.with_extension("corrupt.bak");
            if let Err(re) = tokio::fs::rename(path, &bak).await {
                tracing::error!(
                    module = "health_store",
                    path = %path.display(),
                    backup = %bak.display(),
                    error = %re,
                    "failed to rename corrupt health state file"
                );
            }
            Err(anyhow::anyhow!(
                "health state at {} is corrupt: {e}",
                path.display()
            ))
        }
    }
}

/// Save worker health state to disk.
pub(crate) fn save_health_state(path: &Path, state: &HealthState) -> Result<()> {
    save_json_file(path, state)?;
    Ok(())
}

/// Look up a field on a health entry. Returns `None` if either the worker
/// entry or the field is missing. Used by all typed `get_health_*` helpers.
fn get_health_value<'a>(
    state: &'a HealthState,
    worker: &str,
    field: &str,
) -> Option<&'a serde_json::Value> {
    state.get(worker).and_then(|v| v.get(field))
}

/// Get a numeric field from a health entry.
pub fn get_health_u32(state: &HealthState, worker: &str, field: &str) -> u32 {
    get_health_value(state, worker, field)
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32
}

/// Get a float field from a health entry.
pub(crate) fn get_health_f64(state: &HealthState, worker: &str, field: &str) -> Option<f64> {
    get_health_value(state, worker, field).and_then(|v| v.as_f64())
}

/// Get the PID for a worker from the persisted health state.
///
/// Returns `Pid::new(0)` on unreadable state; the error is logged at WARN.
pub fn get_pid_for_worker(worker: &str) -> crate::Pid {
    let health_path = crate::config::worker_health_path();
    match load_health_state(&health_path) {
        Ok(state) => crate::Pid::new(get_health_u32(&state, worker, "pid")),
        Err(e) => {
            tracing::warn!(
                module = "health_store",
                worker = %worker,
                error = %e,
                "get_pid_for_worker: load_health_state failed"
            );
            crate::Pid::new(0)
        }
    }
}

/// Load health state, set a field, and save back. Logs on failure.
pub(crate) fn persist_health_field(
    worker: &str,
    field: &str,
    value: serde_json::Value,
    err_msg: &str,
) {
    let health_path = crate::config::worker_health_path();
    let mut state = match load_health_state(&health_path) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!(module = "health_store", worker = %worker, error = %e, "{err_msg}");
            return;
        }
    };
    set_health_field(&mut state, worker, field, value);
    if let Err(e) = save_health_state(&health_path, &state) {
        tracing::error!(module = "health_store", worker = %worker, error = %e, "{err_msg}");
    }
}

/// Get a string field from a health entry.
pub(crate) fn get_health_str(state: &HealthState, worker: &str, field: &str) -> Option<String> {
    get_health_value(state, worker, field)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Update a health entry field.
pub(crate) fn set_health_field(
    state: &mut HealthState,
    worker: &str,
    field: &str,
    value: serde_json::Value,
) {
    let entry = state
        .entry(worker.to_string())
        .or_insert_with(|| serde_json::json!({}));
    if let Some(obj) = entry.as_object_mut() {
        obj.insert(field.to_string(), value);
    }
}
