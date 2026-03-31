//! Tick helpers — orphan worker cleanup.
//!
//! Action loop detection has been removed — the unified intervention budget
//! (max_interventions) handles runaway actions. Only `kill_orphan_workers`
//! remains, used in §1 (LOAD) of the tick to clean up stale processes.

use mando_types::task::ItemStatus;

use crate::io::health_store;

/// Kill workers tracked in health state that have no matching in-progress task.
/// Also terminates associated sessions via `terminate_session`.
pub(super) async fn kill_orphan_workers(
    items: &[mando_types::TaskRouting],
    health_state: &mut health_store::HealthState,
    pool: &sqlx::SqlitePool,
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
        // Find the running session for this worker and terminate it.
        match mando_db::queries::sessions::list_running_sessions(pool).await {
            Ok(rows) => {
                if let Some(row) = rows
                    .iter()
                    .find(|r| r.worker_name.as_deref() == Some(&worker_name))
                {
                    tracing::warn!(
                        module = "captain",
                        worker = %worker_name,
                        session_id = %row.session_id,
                        "terminating orphan worker session"
                    );
                    crate::io::session_terminate::terminate_session(
                        pool,
                        &row.session_id,
                        mando_types::SessionStatus::Stopped,
                        Some(health_state),
                    )
                    .await;
                    continue;
                }
                // No matching session — safe to clean health state.
                health_state.remove(&worker_name);
                tracing::info!(
                    module = "captain",
                    worker = %worker_name,
                    "removed orphan health entry (no session)"
                );
            }
            Err(e) => {
                // DB error — skip this orphan, retry on next tick.
                tracing::warn!(
                    module = "captain",
                    worker = %worker_name,
                    error = %e,
                    "failed to query sessions for orphan — skipping, will retry next tick"
                );
            }
        }
    }
}
