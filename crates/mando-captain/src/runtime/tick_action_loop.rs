//! Tick helpers — orphan worker cleanup.
//!
//! Action loop detection has been removed — the unified intervention budget
//! (max_interventions) handles runaway actions. Only `kill_orphan_workers`
//! remains, used in §1 (LOAD) of the tick to clean up stale processes.

use mando_types::task::ItemStatus;

use crate::io::health_store;

/// Kill workers tracked in health state that have no matching in-progress task.
pub(super) async fn kill_orphan_workers(
    items: &[mando_types::TaskRouting],
    health_state: &mut health_store::HealthState,
) {
    let active_workers: std::collections::HashSet<&str> = items
        .iter()
        .filter(|it| it.status == ItemStatus::InProgress)
        .filter_map(|it| it.worker.as_deref())
        .collect();

    let orphan_keys: Vec<String> = health_state
        .keys()
        .filter(|k| !active_workers.contains(k.as_str()))
        // Rebase workers are managed by mergeability phase, not dispatch.
        // They run on pending-review items, so they won't appear in active_workers.
        .filter(|k| !k.starts_with("mando-rebase-"))
        .cloned()
        .collect();

    for worker_name in orphan_keys {
        let pid = health_store::get_health_u32(health_state, &worker_name, "pid");
        if pid > 0 && mando_cc::is_process_alive(pid) {
            tracing::warn!(
                module = "captain",
                worker = %worker_name,
                pid = pid,
                "killing orphan worker"
            );
            if let Err(e) = mando_cc::kill_process(pid).await {
                tracing::warn!(module = "captain", worker = %worker_name, pid = pid, error = %e, "failed to kill orphan worker");
            }
        }
        health_state.remove(&worker_name);
        tracing::info!(module = "captain", worker = %worker_name, "removed orphan health entry");
    }
}
