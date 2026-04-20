//! Health-state persistence for nudge operations.

use anyhow::{Context, Result};

pub(crate) fn persist_nudge_health(
    session_id: &str,
    worker: &str,
    pid: crate::Pid,
    stream_size_before: u64,
    new_count: u32,
    reason: Option<&str>,
) -> Result<()> {
    crate::io::pid_registry::register(session_id, pid)?;
    let health_path = crate::config::worker_health_path();
    let mut hstate = crate::io::health_store::load_health_state(&health_path)
        .with_context(|| format!("load health state from {}", health_path.display()))?;
    crate::io::health_store::set_health_field(
        &mut hstate,
        worker,
        "pid",
        serde_json::json!(pid.as_u32()),
    );
    crate::io::health_store::set_health_field(
        &mut hstate,
        worker,
        "stream_size_at_spawn",
        serde_json::json!(stream_size_before),
    );
    crate::io::health_store::set_health_field(
        &mut hstate,
        worker,
        "nudge_count",
        serde_json::json!(new_count),
    );
    // Track nudge reason for circuit breaker.
    if let Some(r) = reason {
        let last_reason =
            crate::io::health_store::get_health_str(&hstate, worker, "last_nudge_reason");
        let prev_consecutive =
            crate::io::health_store::get_health_u32(&hstate, worker, "nudge_reason_consecutive");
        let consecutive = if last_reason.as_deref() == Some(r) {
            prev_consecutive + 1
        } else {
            1
        };
        crate::io::health_store::set_health_field(
            &mut hstate,
            worker,
            "last_nudge_reason",
            serde_json::json!(r),
        );
        crate::io::health_store::set_health_field(
            &mut hstate,
            worker,
            "nudge_reason_consecutive",
            serde_json::json!(consecutive),
        );
    }
    if let Err(e) = crate::io::health_store::save_health_state(&health_path, &hstate) {
        tracing::error!(module = "captain", worker = %worker, error = %e, "failed to persist health state");
    }
    Ok(())
}
