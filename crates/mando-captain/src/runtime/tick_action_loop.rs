//! Tick helpers — orphan worker cleanup.
//!
//! Action loop detection has been removed — the unified intervention budget
//! (max_interventions) handles runaway actions. Only `kill_orphan_workers`
//! remains, used in §1 (LOAD) of the tick to clean up stale processes.

use mando_types::task::ItemStatus;

use crate::io::health_store;

/// Kill workers tracked in health state that have no matching in-progress task.
/// Also terminates associated sessions via `terminate_session`.
///
/// Queries running sessions once, then terminates all orphans in parallel.
pub(super) async fn kill_orphan_workers(
    items: &[mando_types::TaskRouting],
    health_state: &mut health_store::HealthState,
    pool: &sqlx::SqlitePool,
) -> Vec<String> {
    let active_workers: std::collections::HashSet<&str> = items
        .iter()
        .filter(|it| {
            it.status == ItemStatus::InProgress || it.status == ItemStatus::CaptainReviewing
        })
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

    if orphan_keys.is_empty() {
        return Vec::new();
    }

    // Query running sessions once (not per orphan).
    let sessions = match mando_db::queries::sessions::list_running_sessions(pool).await {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!(
                module = "captain",
                error = %e,
                "failed to query sessions for orphan cleanup — skipping, will retry next tick"
            );
            return Vec::new();
        }
    };

    // Collect orphans with running sessions for parallel termination.
    struct OrphanTerminate<'a> {
        worker_name: &'a str,
        session_id: &'a str,
    }
    let mut to_terminate: Vec<OrphanTerminate> = Vec::new();

    for worker_name in &orphan_keys {
        if let Some(row) = sessions
            .iter()
            .find(|r| r.worker_name.as_deref() == Some(worker_name.as_str()))
        {
            tracing::warn!(
                module = "captain",
                worker = %worker_name,
                session_id = %row.session_id,
                "terminating orphan worker session"
            );
            to_terminate.push(OrphanTerminate {
                worker_name,
                session_id: &row.session_id,
            });
        } else {
            health_state.remove(worker_name);
            tracing::info!(
                module = "captain",
                worker = %worker_name,
                "removed orphan health entry (no session)"
            );
        }
    }

    // Terminate all orphan sessions in parallel.
    if !to_terminate.is_empty() {
        let futs: Vec<_> = to_terminate
            .iter()
            .map(|o| {
                crate::io::session_terminate::terminate_session(
                    pool,
                    o.session_id,
                    mando_types::SessionStatus::Stopped,
                    None,
                )
            })
            .collect();
        futures::future::join_all(futs).await;

        // Clean health state after parallel termination.
        for o in &to_terminate {
            health_state.remove(o.worker_name);
        }
    }

    orphan_keys
}
