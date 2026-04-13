//! Startup reconciliation tasks.

use sqlx::SqlitePool;

use crate::routes_scout::spawn_scout_processing;
use crate::AppState;

/// Clean up stale state on startup: dead PIDs, stuck scout items, and
/// orphan research runs from interrupted daemon runs.
pub async fn startup_reconciliation(pool: &SqlitePool) {
    if let Err(e) = mando_captain::io::pid_registry::cleanup_dead() {
        tracing::warn!(module = "startup", error = %e, "pid_registry cleanup_dead failed");
    }
    match mando_db::queries::scout::reset_stale_fetched(pool).await {
        Ok(0) => {}
        Ok(n) => tracing::info!(
            module = "startup",
            count = n,
            "reset stale fetched scout items"
        ),
        Err(e) => {
            tracing::warn!(module = "startup", error = %e, "failed to reset stale fetched items")
        }
    }
    match mando_db::queries::scout_research::reset_stale_running(pool).await {
        Ok(0) => {}
        Ok(n) => tracing::info!(
            module = "startup",
            count = n,
            "marked stale running research runs as failed"
        ),
        Err(e) => {
            tracing::warn!(module = "startup", error = %e, "failed to reset stale running research runs")
        }
    }
}

/// Re-queue any scout items left in `pending` from a prior daemon run.
/// Must be called after `AppState` is constructed so we can spawn tasks.
pub async fn resume_pending_scout_items(state: &AppState) {
    let pool = state.db.pool();
    match mando_db::queries::scout::list_processable(pool).await {
        Ok(items) if items.is_empty() => {}
        Ok(items) => {
            let count = items.len();
            for item in items {
                spawn_scout_processing(state, item.id, item.url);
            }
            tracing::info!(
                module = "startup",
                count,
                "resumed pending scout items for processing"
            );
        }
        Err(e) => {
            tracing::warn!(module = "startup", error = %e, "failed to query pending scout items")
        }
    }
}
