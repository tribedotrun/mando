//! Startup reconciliation tasks.

use sqlx::SqlitePool;

/// Clean up stale state on startup: dead PIDs, stuck scout items, and
/// orphan research runs from interrupted daemon runs.
pub async fn startup_reconciliation(pool: &SqlitePool) {
    if let Err(e) = captain::cleanup_pid_on_startup().await {
        // Escalated to error: a failure here means orphan subprocesses
        // from a prior daemon may still be running, which downstream
        // session reconciliation cannot detect on its own.
        tracing::error!(module = "startup", error = %e, "pid_registry cleanup_on_startup failed");
    }
    match scout::reset_stale_fetched(pool).await {
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
    match scout::reset_stale_running(pool).await {
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
