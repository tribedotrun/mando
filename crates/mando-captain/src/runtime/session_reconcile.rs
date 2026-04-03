//! Session status reconciliation — safety net for stale "running" sessions.
//!
//! Three-layer detection on each tick (PID-first to prevent resume race):
//! L1: PID liveness — alive = skip, dead = terminate
//! L2: Stream result — found after last init = terminate
//! L3: Stream staleness — stale beyond threshold = terminate

use mando_types::SessionStatus;
use sqlx::SqlitePool;

/// Reconcile all sessions currently marked "running" against PID + stream truth.
pub(crate) async fn reconcile_running_sessions(pool: &SqlitePool, stale_threshold_s: f64) {
    let running = match mando_db::queries::sessions::list_running_sessions(pool).await {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!(module = "captain", error = %e, "session reconciliation: failed to query");
            return;
        }
    };

    if running.is_empty() {
        return;
    }

    let mut reconciled = 0u32;
    for row in &running {
        let sid = &row.session_id;

        // L1: PID liveness
        if let Some(pid) = crate::io::pid_registry::get_pid(sid) {
            if pid > 0 {
                if mando_cc::is_process_alive(pid) {
                    continue; // genuinely running
                }
                // Dead PID — terminate
                crate::io::session_terminate::terminate_session(
                    pool,
                    sid,
                    SessionStatus::Stopped,
                    None,
                )
                .await;
                reconciled += 1;
                continue;
            }
        }

        // L2: Stream result
        let stream_path = mando_config::stream_path_for_session(sid);
        if let Some(result) = mando_cc::get_stream_result(&stream_path) {
            let status = if mando_cc::is_clean_result(&result) {
                SessionStatus::Stopped
            } else {
                // Check if the failure was caused by rate limiting — activate
                // cooldown so the captain tick pauses before retrying.
                if super::rate_limit_cooldown::check_and_activate_from_stream(sid) {
                    tracing::info!(
                        module = "captain",
                        session_id = %sid,
                        "session reconciliation detected rate limit — cooldown activated"
                    );
                }
                SessionStatus::Failed
            };
            crate::io::session_terminate::terminate_session(pool, sid, status, None).await;
            reconciled += 1;
            continue;
        }

        // L3: Stream staleness
        if let Some(stale_secs) = mando_cc::stream_stale_seconds(&stream_path) {
            if stale_secs > stale_threshold_s {
                crate::io::session_terminate::terminate_session(
                    pool,
                    sid,
                    SessionStatus::Stopped,
                    None,
                )
                .await;
                reconciled += 1;
                continue;
            }
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
