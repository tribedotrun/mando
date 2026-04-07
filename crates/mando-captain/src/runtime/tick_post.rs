//! §5 POST — persist health, prune WAL, SSE, summary.

use std::path::Path;

use anyhow::{Context, Result};

use mando_shared::EventBus;
use mando_types::BusEvent;

use crate::biz::tick_logic;
use crate::io::{health_store, health_store::HealthState, ops_log};

/// Persist health state, prune stale WAL entries, flush notifications,
/// and publish SSE events. Called at the end of every non-dry-run tick.
pub(crate) async fn run_post_phase(
    dry_run: bool,
    health_path: &Path,
    health_state: &HealthState,
    removed_workers: &[String],
    notifier: &super::notify::Notifier,
    bus: Option<&EventBus>,
) -> Result<()> {
    if !dry_run {
        let mut fresh = health_store::load_health_state(health_path)
            .with_context(|| format!("load health state from {}", health_path.display()))?;
        merge_health_state(&mut fresh, health_state, removed_workers);
        if let Err(e) = health_store::save_health_state(health_path, &fresh) {
            tracing::warn!(module = "captain", error = %e, "health state save failed — worker tracking may be stale");
        }

        // Prune stale WAL entries (older than 72 hours).
        let wal_path = ops_log::ops_log_path();
        let mut wal = ops_log::load_ops_log(&wal_path);
        ops_log::prune_stale(&mut wal, ops_log::STALE_AGE_SECS);
        ops_log::save_ops_log(&wal, &wal_path).with_context(|| {
            format!(
                "tick post phase: failed to save ops log at {}",
                wal_path.display()
            )
        })?;
    }

    // Flush batched notifications.
    notifier.flush_batch().await;

    // SSE publish — notify UI of state changes.
    if let Some(bus) = bus {
        bus.send(BusEvent::Tasks, None);
        bus.send(
            BusEvent::Status,
            Some(serde_json::json!({"action": "tick"})),
        );
        bus.send(BusEvent::Sessions, None);
    }

    Ok(())
}

/// Merge in-memory health state into the on-disk snapshot.
///
/// Only tick-owned fields (`cpu_time_s`, `cwd`) are overlaid from the
/// in-memory snapshot. All other fields (written to disk by §4 EXECUTE)
/// are preserved from the on-disk version. Workers explicitly removed
/// during the tick (orphan cleanup) are removed from the merged result;
/// other on-disk entries are preserved even if absent from in-memory,
/// because they may have been created during §4 (e.g. newly dispatched
/// workers).
pub(crate) fn merge_health_state(
    on_disk: &mut HealthState,
    in_memory: &HealthState,
    removed_workers: &[String],
) {
    const TICK_OWNED_FIELDS: &[&str] = &["cpu_time_s", "cwd"];

    for (worker, entry) in in_memory {
        if let Some(obj) = entry.as_object() {
            for (k, v) in obj {
                if TICK_OWNED_FIELDS.contains(&k.as_str()) {
                    health_store::set_health_field(on_disk, worker, k, v.clone());
                }
            }
        }
    }

    // Only remove workers that were explicitly cleaned up during the tick.
    for w in removed_workers {
        on_disk.remove(w);
    }
}

/// Archive terminal tasks and reconcile stale sessions.
pub(crate) async fn run_post_cleanup(
    dry_run: bool,
    store_lock: &std::sync::Arc<tokio::sync::RwLock<crate::io::task_store::TaskStore>>,
    workflow: &mando_config::workflow::CaptainWorkflow,
    alerts: &mut Vec<String>,
) {
    if dry_run {
        return;
    }
    // Archive terminal tasks that have been finalized longer than the grace period.
    {
        let store = store_lock.read().await;
        match store
            .archive_terminal(workflow.agent.archive_grace_secs)
            .await
        {
            Ok(n) if n > 0 => {
                tracing::info!(module = "captain", archived = n, "archived terminal tasks");
            }
            Err(e) => {
                tracing::warn!(module = "captain", error = %e, "archive terminal tasks failed");
            }
            _ => {}
        }
    }
    // Reconcile stale "running" sessions against stream ground truth.
    {
        let store = store_lock.read().await;
        super::session_reconcile::reconcile_running_sessions(
            store.pool(),
            workflow.agent.stale_threshold_s,
            alerts,
        )
        .await;
    }
}

/// Build tick summary from status counts and log it.
pub(crate) fn log_tick_summary(
    status_counts: &std::collections::HashMap<String, usize>,
    active_workers: usize,
    alert_count: usize,
) {
    let summary = tick_logic::format_status_summary(status_counts);
    tracing::info!(
        module = "captain",
        active_workers = active_workers,
        tasks = %summary,
        alert_count = alert_count,
        "tick done"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_health(fields: &[(&str, &str, serde_json::Value)]) -> HealthState {
        let mut state = HealthState::new();
        for (worker, field, value) in fields {
            health_store::set_health_field(&mut state, worker, field, value.clone());
        }
        state
    }

    #[test]
    fn pending_ai_feedback_cleared_on_disk_survives_merge() {
        let in_memory = make_health(&[
            ("w1", "pending_ai_feedback", serde_json::json!("fix CI")),
            ("w1", "cpu_time_s", serde_json::json!(42.0)),
            ("w1", "pid", serde_json::json!(1234)),
        ]);
        let mut on_disk = make_health(&[
            ("w1", "pid", serde_json::json!(5678)),
            ("w1", "nudge_count", serde_json::json!(5)),
        ]);

        merge_health_state(&mut on_disk, &in_memory, &[]);

        let cpu = health_store::get_health_f64(&on_disk, "w1", "cpu_time_s");
        assert_eq!(cpu, Some(42.0));
        let fb = health_store::get_health_str(&on_disk, "w1", "pending_ai_feedback");
        assert!(fb.is_none(), "pending_ai_feedback was clobbered: {fb:?}");
        let pid = health_store::get_health_u32(&on_disk, "w1", "pid");
        assert_eq!(pid, 5678);
        let nc = health_store::get_health_u32(&on_disk, "w1", "nudge_count");
        assert_eq!(nc, 5);
    }

    #[test]
    fn nudge_reason_fields_not_clobbered() {
        let in_memory = make_health(&[
            ("w1", "last_nudge_reason", serde_json::json!("old reason")),
            ("w1", "nudge_reason_consecutive", serde_json::json!(1)),
        ]);
        let mut on_disk = make_health(&[
            ("w1", "last_nudge_reason", serde_json::json!("new reason")),
            ("w1", "nudge_reason_consecutive", serde_json::json!(3)),
        ]);

        merge_health_state(&mut on_disk, &in_memory, &[]);

        let reason = health_store::get_health_str(&on_disk, "w1", "last_nudge_reason");
        assert_eq!(reason.as_deref(), Some("new reason"));
        let consec = health_store::get_health_u32(&on_disk, "w1", "nudge_reason_consecutive");
        assert_eq!(consec, 3);
    }

    #[test]
    fn orphan_worker_removed_from_merged_state() {
        let in_memory = make_health(&[("w1", "cpu_time_s", serde_json::json!(10.0))]);
        let mut on_disk = make_health(&[
            ("w1", "pid", serde_json::json!(1111)),
            ("orphan", "pid", serde_json::json!(9999)),
        ]);
        let removed = vec!["orphan".to_string()];

        merge_health_state(&mut on_disk, &in_memory, &removed);

        assert!(on_disk.contains_key("w1"), "live worker should survive");
        assert!(
            !on_disk.contains_key("orphan"),
            "orphan worker should be removed"
        );
    }

    #[test]
    fn new_worker_on_disk_preserved() {
        // A worker written to disk during §4 (dispatch) that wasn't in the
        // §1 snapshot should survive the merge.
        let in_memory = make_health(&[("w1", "cpu_time_s", serde_json::json!(10.0))]);
        let mut on_disk = make_health(&[
            ("w1", "pid", serde_json::json!(1111)),
            ("w2-new", "pid", serde_json::json!(2222)),
        ]);

        merge_health_state(&mut on_disk, &in_memory, &[]);

        assert!(on_disk.contains_key("w1"));
        assert!(
            on_disk.contains_key("w2-new"),
            "new worker written during §4 should be preserved"
        );
    }

    #[test]
    fn cwd_is_overlaid_from_in_memory() {
        let in_memory = make_health(&[("w1", "cwd", serde_json::json!("/new/path"))]);
        let mut on_disk = make_health(&[("w1", "cwd", serde_json::json!("/old/path"))]);

        merge_health_state(&mut on_disk, &in_memory, &[]);

        let cwd = health_store::get_health_str(&on_disk, "w1", "cwd");
        assert_eq!(cwd.as_deref(), Some("/new/path"));
    }
}
