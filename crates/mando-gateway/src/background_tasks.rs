//! Background task spawners — captain auto-tick loop.

use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicBool, Ordering};

use futures_util::FutureExt;
use tracing::info;

use crate::AppState;

/// Number of consecutive auto-tick failures that triggers the degraded flag
/// and a user-visible notification.
const DEGRADED_FAILURE_THRESHOLD: u32 = 5;

/// Set to true when auto-tick has failed
/// [`DEGRADED_FAILURE_THRESHOLD`] consecutive times. Cleared on the next
/// successful tick. Exposed via `/api/health/system`.
static CAPTAIN_HEALTH_DEGRADED: AtomicBool = AtomicBool::new(false);

/// Returns whether the captain auto-tick loop has flagged itself as degraded
/// (see [`DEGRADED_FAILURE_THRESHOLD`]).
pub fn captain_health_degraded() -> bool {
    CAPTAIN_HEALTH_DEGRADED.load(Ordering::Relaxed)
}

/// Spawn the captain auto-tick loop that periodically runs a captain tick
/// and cleans up expired CC sessions.
pub fn spawn_auto_tick(state: &AppState, tick_interval_s: u64) {
    let tick_config = state.config.clone();
    let tick_workflow = state.captain_workflow.clone();
    let tick_bus = state.bus.clone();
    let tick_store = state.task_store.clone();
    let tick_sessions = state.cc_session_mgr.clone();
    let cancel_outer = state.cancellation_token.clone();
    let interval = std::time::Duration::from_secs(tick_interval_s);

    info!(
        module = "captain",
        interval_s = tick_interval_s,
        "auto-tick enabled"
    );

    state.task_tracker.spawn(async move {
        // Initial delay to let the server start. Abort early if a shutdown
        // signal arrives during the warm-up period.
        tokio::select! {
            _ = tokio::time::sleep(std::time::Duration::from_secs(5)) => {}
            _ = cancel_outer.cancelled() => {
                info!(module = "captain", "auto-tick cancelled before first run");
                return;
            }
        }
        // Outer loop: restart on panic so captain never stops permanently.
        loop {
            if cancel_outer.is_cancelled() {
                info!(module = "captain", "auto-tick exiting on cancellation");
                return;
            }
            let tick_config = tick_config.clone();
            let tick_workflow = tick_workflow.clone();
            let tick_bus = tick_bus.clone();
            let tick_store = tick_store.clone();
            let tick_sessions = tick_sessions.clone();
            let cancel = cancel_outer.clone();

            let result = AssertUnwindSafe(async move {
                let mut consecutive_failures: u32 = 0;
                loop {
                    // Cleanup expired CC sessions (ask, etc.).
                    let expired = tick_sessions.cleanup_expired();
                    if expired > 0 {
                        info!(
                            module = "cc-session",
                            expired = expired,
                            "expired sessions cleaned up"
                        );
                    }

                    let cfg = tick_config.load_full();
                    if cfg.captain.auto_schedule {
                        let wf = tick_workflow.load_full();
                        match mando_captain::runtime::dashboard::trigger_captain_tick(
                            &cfg,
                            &wf,
                            false,
                            Some(&tick_bus),
                            true,
                            &tick_store,
                        )
                        .await
                        {
                            Ok(val) => {
                                if consecutive_failures >= DEGRADED_FAILURE_THRESHOLD {
                                    tracing::info!(
                                        module = "captain",
                                        "auto-tick recovered — clearing degraded flag"
                                    );
                                }
                                consecutive_failures = 0;
                                CAPTAIN_HEALTH_DEGRADED.store(false, Ordering::Relaxed);
                                let workers = val
                                    .get("active_workers")
                                    .and_then(|v| v.as_u64())
                                    .unwrap_or(0);
                                let tasks = val.get("tasks");
                                info!(module = "captain", workers = workers, tasks = ?tasks, "auto-tick completed");
                            }
                            Err(e) => {
                                consecutive_failures += 1;
                                tracing::warn!(
                                    module = "captain",
                                    error = %e,
                                    consecutive_failures,
                                    "auto-tick failed"
                                );
                                // Flip the degraded flag once, on the exact
                                // crossing, and emit a notification the UI
                                // can surface. Subsequent failures keep
                                // logging but don't re-spam.
                                if consecutive_failures == DEGRADED_FAILURE_THRESHOLD {
                                    CAPTAIN_HEALTH_DEGRADED.store(true, Ordering::Relaxed);
                                    tracing::error!(
                                        module = "captain",
                                        consecutive_failures,
                                        "auto-tick has failed repeatedly — marking degraded"
                                    );
                                    let payload = mando_types::events::NotificationPayload {
                                        message: format!(
                                            "\u{26a0}\u{fe0f} Captain auto-tick failing ({consecutive_failures} consecutive failures). Last error: {e}"
                                        ),
                                        level: mando_types::NotifyLevel::Normal,
                                        kind: mando_types::events::NotificationKind::Generic,
                                        task_key: Some("captain:degraded".into()),
                                        reply_markup: None,
                                    };
                                    match serde_json::to_value(&payload) {
                                        Ok(json) => tick_bus.send(
                                            mando_types::BusEvent::Notification,
                                            Some(json),
                                        ),
                                        Err(ser_err) => tracing::error!(
                                            module = "captain",
                                            error = %ser_err,
                                            "failed to serialize degraded notification"
                                        ),
                                    }
                                } else if consecutive_failures > DEGRADED_FAILURE_THRESHOLD
                                    && consecutive_failures.is_multiple_of(10)
                                {
                                    tracing::error!(
                                        module = "captain",
                                        consecutive_failures,
                                        "auto-tick failing repeatedly — captain still degraded"
                                    );
                                }
                            }
                        }
                    }
                    // Wait for either the scheduled interval OR a worker exit signal.
                    // Worker exit triggers an immediate tick so state transitions happen
                    // within milliseconds instead of waiting up to 30s.
                    // Also exit promptly on shutdown cancellation.
                    tokio::select! {
                        _ = tokio::time::sleep(interval) => {},
                        _ = mando_captain::WORKER_EXIT_SIGNAL.notified() => {
                            tracing::debug!(module = "captain", "worker exit detected — triggering immediate tick");
                        },
                        _ = cancel.cancelled() => {
                            info!(module = "captain", "auto-tick cancelled mid-loop");
                            return;
                        },
                    }
                }
            })
            .catch_unwind()
            .await;

            if let Err(panic) = result {
                tracing::error!(
                    module = "captain",
                    "auto-tick loop panicked — restarting in 5s: {:?}",
                    panic
                );
                tokio::select! {
                    _ = tokio::time::sleep(std::time::Duration::from_secs(5)) => {}
                    _ = cancel_outer.cancelled() => {
                        info!(module = "captain", "auto-tick cancelled during panic backoff");
                        return;
                    }
                }
            }
        }
    });
}
