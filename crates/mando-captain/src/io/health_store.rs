//! Worker health state read/write — `~/.mando/state/worker-health.json`.

use std::collections::HashMap;
use std::path::Path;

use anyhow::Result;
use mando_shared::{load_json_file, save_json_file};

/// Worker health entry.
pub type HealthState = HashMap<String, serde_json::Value>;

/// Load worker health state from disk.
pub fn load_health_state(path: &Path) -> HealthState {
    load_json_file(path, "health_store")
}

/// Save worker health state to disk.
pub(crate) fn save_health_state(path: &Path, state: &HealthState) -> Result<()> {
    save_json_file(path, state)
}

/// Get a numeric field from a health entry.
pub fn get_health_u32(state: &HealthState, worker: &str, field: &str) -> u32 {
    state
        .get(worker)
        .and_then(|v| v.get(field))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32
}

/// Get a u64 field from a health entry.
#[cfg(test)]
pub(crate) fn get_health_u64(state: &HealthState, worker: &str, field: &str) -> u64 {
    state
        .get(worker)
        .and_then(|v| v.get(field))
        .and_then(|v| v.as_u64())
        .unwrap_or(0)
}

/// Get a float field from a health entry.
pub(crate) fn get_health_f64(state: &HealthState, worker: &str, field: &str) -> Option<f64> {
    state
        .get(worker)
        .and_then(|v| v.get(field))
        .and_then(|v| v.as_f64())
}

/// Get the PID for a worker from the persisted health state.
pub fn get_pid_for_worker(worker: &str) -> u32 {
    let health_path = mando_config::worker_health_path();
    let state = load_health_state(&health_path);
    get_health_u32(&state, worker, "pid")
}

/// Persist a worker's PID to the health state file (load → set → save).
pub fn persist_worker_pid(worker: &str, pid: u32) {
    let health_path = mando_config::worker_health_path();
    let mut state = load_health_state(&health_path);
    set_health_field(&mut state, worker, "pid", serde_json::json!(pid));
    if let Err(e) = save_health_state(&health_path, &state) {
        tracing::error!(
            module = "health_store",
            worker = %worker,
            pid = pid,
            error = %e,
            "failed to persist worker PID — process liveness checks will fail"
        );
    }
}

/// Persist a worker's nudge count to the health state file (load → set → save).
pub(crate) fn persist_nudge_count(worker: &str, count: u32) {
    let health_path = mando_config::worker_health_path();
    let mut state = load_health_state(&health_path);
    set_health_field(&mut state, worker, "nudge_count", serde_json::json!(count));
    if let Err(e) = save_health_state(&health_path, &state) {
        tracing::error!(
            module = "health_store",
            worker = %worker,
            count = count,
            error = %e,
            "failed to persist nudge count — escalation threshold may reset on restart"
        );
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_state() {
        let state = HealthState::new();
        assert_eq!(get_health_u32(&state, "w", "nudge_count"), 0);
        assert_eq!(get_health_u64(&state, "w", "stream_size_at_spawn"), 0);
        assert!(get_health_f64(&state, "w", "cpu_time_s").is_none());
    }

    #[test]
    fn u64_round_trip() {
        let mut state = HealthState::new();
        let big: u64 = 1_000_000;
        set_health_field(
            &mut state,
            "w",
            "stream_size_at_spawn",
            serde_json::json!(big),
        );
        assert_eq!(get_health_u64(&state, "w", "stream_size_at_spawn"), big);
    }

    #[test]
    fn set_and_get() {
        let mut state = HealthState::new();
        set_health_field(&mut state, "w", "nudge_count", serde_json::json!(5));
        assert_eq!(get_health_u32(&state, "w", "nudge_count"), 5);
    }

    #[test]
    fn save_load_round_trip() {
        let tmp = std::env::temp_dir().join("mando-test-health.json");
        let mut state = HealthState::new();
        set_health_field(&mut state, "w", "cpu", serde_json::json!(42.5));
        save_health_state(&tmp, &state).unwrap();

        let loaded = load_health_state(&tmp);
        assert_eq!(get_health_f64(&loaded, "w", "cpu"), Some(42.5));

        std::fs::remove_file(&tmp).ok();
    }
}
