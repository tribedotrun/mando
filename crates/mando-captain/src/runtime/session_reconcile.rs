//! Session status reconciliation — safety net for stale "running" sessions.
//!
//! Three-layer detection on each tick (PID-first to prevent resume race):
//! L1: PID liveness — alive = skip, dead = terminate
//! L2: Stream result — found after last init = terminate
//! L3: Stream staleness — stale beyond threshold = terminate
//!
//! All detection is synchronous; terminations run in parallel via `join_all`.

use mando_types::SessionStatus;
use sqlx::SqlitePool;

/// Reconcile all sessions currently marked "running" against PID + stream truth.
///
/// Reconcile failures (DB query error) are pushed into `alerts` so the tick
/// surfaces them alongside other problems instead of burying them in logs.
pub(crate) async fn reconcile_running_sessions(
    pool: &SqlitePool,
    stale_threshold: std::time::Duration,
    alerts: &mut Vec<String>,
) {
    let stale_threshold_s = stale_threshold.as_secs_f64();
    let running = match mando_db::queries::sessions::list_running_sessions(pool).await {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!(module = "captain", error = %e, "session reconciliation: failed to query");
            alerts.push(format!("session reconciliation query failed: {e}"));
            return;
        }
    };

    if running.is_empty() {
        return;
    }

    // Phase 1: Determine which sessions need termination (all checks are sync).
    struct TermJob {
        session_id: String,
        status: SessionStatus,
    }
    let mut jobs: Vec<TermJob> = Vec::new();

    for row in &running {
        let sid = &row.session_id;

        // L1: PID liveness
        if let Some(pid) = crate::io::pid_registry::get_pid(sid) {
            if pid.as_u32() > 0 {
                if mando_cc::is_process_alive(pid) {
                    // L0: Live rate-limit detection — kill the process so it
                    // gets reopened on the next tick with a healthy credential.
                    let stream_path = mando_config::stream_path_for_session(sid);
                    if mando_cc::has_rate_limit_rejection(&stream_path).is_some()
                        && super::credential_rate_limit::check_and_activate_from_stream(pool, sid)
                            .await
                    {
                        tracing::warn!(
                            module = "captain",
                            session_id = %sid,
                            ?pid,
                            "live session hit rate limit — killing process so tick reopens with new credential"
                        );
                        if let Err(e) = mando_cc::kill_process(pid).await {
                            tracing::warn!(
                                module = "captain",
                                session_id = %sid,
                                error = %e,
                                "failed to kill rate-limited process"
                            );
                        }
                        jobs.push(TermJob {
                            session_id: sid.clone(),
                            status: SessionStatus::Failed,
                        });
                        continue;
                    }
                    continue; // genuinely running
                }
                jobs.push(TermJob {
                    session_id: sid.clone(),
                    status: SessionStatus::Stopped,
                });
                continue;
            }
        }

        // L2: Stream result
        let stream_path = mando_config::stream_path_for_session(sid);
        if let Some(result) = mando_cc::get_stream_result(&stream_path) {
            let status = if mando_cc::is_clean_result(&result) {
                SessionStatus::Stopped
            } else {
                if super::credential_rate_limit::check_and_activate_from_stream(pool, sid).await {
                    tracing::info!(
                        module = "captain",
                        session_id = %sid,
                        "session reconciliation detected rate limit — cooldown activated"
                    );
                }
                SessionStatus::Failed
            };
            jobs.push(TermJob {
                session_id: sid.clone(),
                status,
            });
            continue;
        }

        // L3: Stream staleness
        if let Some(stale_secs) = mando_cc::stream_stale_seconds(&stream_path) {
            if stale_secs > stale_threshold_s {
                jobs.push(TermJob {
                    session_id: sid.clone(),
                    status: SessionStatus::Stopped,
                });
            }
        }
    }

    if jobs.is_empty() {
        return;
    }

    // Log rate-limit status for every terminated session (covers workers
    // which don't go through the Notifier's check_rate_limit path).
    for job in &jobs {
        let stream_path = mando_config::stream_path_for_session(&job.session_id);
        if let Some(rl) = mando_cc::last_rate_limit_status(&stream_path) {
            let cred_id = mando_db::queries::sessions::get_credential_id(pool, &job.session_id)
                .await
                .unwrap_or(None);
            tracing::info!(
                module = "captain",
                session_id = %job.session_id,
                credential_id = ?cred_id,
                rl_status = %rl.status,
                rl_type = rl.rate_limit_type.as_deref().unwrap_or("unknown"),
                resets_at = ?rl.resets_at,
                utilization = ?rl.utilization,
                overage = rl.overage_status.as_deref().unwrap_or("none"),
                "session rate-limit status at exit"
            );
        }
    }

    // Phase 2: Terminate all in parallel.
    let reconciled = jobs.len();
    let futs: Vec<_> = jobs
        .iter()
        .map(|job| {
            crate::io::session_terminate::terminate_session(pool, &job.session_id, job.status, None)
        })
        .collect();
    futures::future::join_all(futs).await;

    tracing::info!(
        module = "captain",
        checked = running.len(),
        reconciled,
        "session reconciliation complete"
    );
}
