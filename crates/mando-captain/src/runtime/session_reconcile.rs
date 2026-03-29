//! Session status reconciliation — safety net for stale "running" sessions.
//!
//! On each tick, queries the DB for sessions marked "running" and checks the
//! stream file for a result event. If the stream has a result but the DB still
//! says "running", updates the DB to "stopped" or "failed".

use mando_types::SessionStatus;
use sqlx::SqlitePool;

/// Reconcile all sessions currently marked "running" against stream ground truth.
pub(crate) async fn reconcile_running_sessions(pool: &SqlitePool) {
    let running = match mando_db::queries::sessions::list_running_sessions(pool).await {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!(module = "captain", error = %e, "session reconciliation: failed to query running sessions");
            return;
        }
    };

    if running.is_empty() {
        return;
    }

    let mut reconciled = 0u32;
    for row in &running {
        let stream_path = mando_config::stream_path_for_session(&row.session_id);
        let Some(result) = mando_cc::get_stream_result(&stream_path) else {
            continue; // No result yet — session is genuinely still running (or has no stream file).
        };

        let new_status = if mando_cc::is_clean_result(&result) {
            SessionStatus::Stopped
        } else {
            SessionStatus::Failed
        };

        if let Err(e) =
            mando_db::queries::sessions::update_session_status(pool, &row.session_id, new_status)
                .await
        {
            tracing::warn!(
                module = "captain",
                session_id = %row.session_id,
                error = %e,
                "session reconciliation: failed to update status"
            );
        } else {
            reconciled += 1;
        }
    }

    if reconciled > 0 {
        tracing::info!(
            module = "captain",
            checked = running.len(),
            reconciled,
            "session reconciliation complete"
        );
    }
}
