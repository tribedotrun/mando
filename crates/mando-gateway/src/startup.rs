//! Startup reconciliation tasks.

use sqlx::SqlitePool;

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
