//! Background task spawners — captain auto-tick and distiller cron loops.

use std::panic::AssertUnwindSafe;
use std::sync::Arc;

use futures_util::FutureExt;
use tracing::info;

use crate::AppState;

/// Spawn the captain auto-tick loop that periodically runs a captain tick
/// and cleans up expired CC/ops sessions.
pub fn spawn_auto_tick(state: &AppState, tick_interval_s: u64) {
    let tick_config = state.config.clone();
    let tick_workflow = state.captain_workflow.clone();
    let tick_bus = state.bus.clone();
    let tick_store = state.task_store.clone();
    let tick_sessions = state.cc_session_mgr.clone();
    let interval = std::time::Duration::from_secs(tick_interval_s);

    info!(
        module = "captain",
        interval_s = tick_interval_s,
        "auto-tick enabled"
    );

    tokio::spawn(async move {
        // Initial delay to let the server start.
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        // Outer loop: restart on panic so captain never stops permanently.
        loop {
            let tick_config = tick_config.clone();
            let tick_workflow = tick_workflow.clone();
            let tick_bus = tick_bus.clone();
            let tick_store = tick_store.clone();
            let tick_sessions = tick_sessions.clone();

            let result = AssertUnwindSafe(async move {
                let mut consecutive_failures: u32 = 0;
                loop {
                    // Cleanup expired ops/ask sessions (runs regardless of auto_schedule).
                    let expired = tick_sessions.write().await.cleanup_expired();
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
                                consecutive_failures = 0;
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
                                if consecutive_failures > 0 && consecutive_failures.is_multiple_of(10) {
                                    tracing::error!(
                                        module = "captain",
                                        consecutive_failures,
                                        "auto-tick failing repeatedly — captain may be degraded"
                                    );
                                }
                            }
                        }
                    }
                    // Wait for either the scheduled interval OR a worker exit signal.
                    // Worker exit triggers an immediate tick so state transitions happen
                    // within milliseconds instead of waiting up to 30s.
                    tokio::select! {
                        _ = tokio::time::sleep(interval) => {},
                        _ = mando_captain::WORKER_EXIT_SIGNAL.notified() => {
                            tracing::debug!(module = "captain", "worker exit detected — triggering immediate tick");
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
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
        }
    });
}

/// Spawn the distiller cron loop that runs the pattern distiller on schedule.
pub fn spawn_distiller_cron(
    config: Arc<arc_swap::ArcSwap<mando_config::Config>>,
    workflow: Arc<arc_swap::ArcSwap<mando_config::CaptainWorkflow>>,
    bus: Arc<mando_shared::EventBus>,
    pool: sqlx::SqlitePool,
    cron_expr: &str,
) {
    let parsed = match mando_shared::CronExpr::parse(cron_expr) {
        Ok(p) => p,
        Err(_) => {
            tracing::warn!(
                module = "distiller",
                cron = %cron_expr,
                "invalid learn_cron_expr — distiller cron disabled"
            );
            return;
        }
    };

    info!(
        module = "distiller",
        cron = %cron_expr,
        "distiller cron enabled"
    );

    tokio::spawn(async move {
        // Outer loop: restart on panic so distiller never stops permanently.
        loop {
            let config = config.clone();
            let workflow = workflow.clone();
            let bus = bus.clone();
            let pool = pool.clone();
            let parsed = parsed.clone();

            let result = AssertUnwindSafe(async move {
                loop {
                    let now = time::OffsetDateTime::now_utc().unix_timestamp();
                    let next = match parsed.next_after(now) {
                        Some(ts) => ts,
                        None => {
                            tracing::error!(module = "distiller", "no next cron run — stopping");
                            break;
                        }
                    };
                    let delay = (next - now).max(60) as u64;
                    info!(
                        module = "distiller",
                        next_in_s = delay,
                        "sleeping until next distiller run"
                    );
                    tokio::time::sleep(std::time::Duration::from_secs(delay)).await;

                    let cfg = config.load_full();
                    if !cfg.captain.auto_schedule {
                        continue;
                    }
                    let wf = workflow.load_full();
                    match mando_captain::runtime::distiller::run_distiller(&cfg, &wf, &pool).await {
                        Ok(result) => {
                            crate::routes_knowledge::notify_patterns(&bus, &result.patterns);
                            info!(
                                module = "distiller",
                                patterns = result.patterns_found,
                                "cron distiller completed: {}",
                                result.summary
                            );
                        }
                        Err(e) => {
                            tracing::warn!(
                                module = "distiller",
                                error = %e,
                                "cron distiller failed"
                            );
                        }
                    }
                }
            })
            .catch_unwind()
            .await;

            if let Err(panic) = result {
                tracing::error!(
                    module = "distiller",
                    "distiller cron loop panicked — restarting in 5s: {:?}",
                    panic
                );
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            } else {
                tracing::info!(
                    module = "distiller",
                    "distiller cron loop exited — stopping"
                );
                break;
            }
        }
    });
}
