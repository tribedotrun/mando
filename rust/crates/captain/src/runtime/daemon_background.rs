use std::panic::AssertUnwindSafe;
use std::time::Duration;

use futures::FutureExt;
use tracing::{info, warn};

use super::{degraded_failure_threshold, CaptainRuntime};

pub(super) fn spawn_auto_tick(runtime: &CaptainRuntime) {
    let settings = runtime.settings().clone();
    let tick_rx = settings.subscribe_tick();
    let tick_bus = runtime.bus().clone();
    let tick_store = runtime.task_store().clone();
    let cancel_outer = runtime.cancellation_token().clone();
    let initial_interval = *tick_rx.borrow();
    let task_tracker = runtime.task_tracker().clone();
    let runtime = runtime.clone();

    info!(
        module = "captain",
        interval_s = initial_interval.as_secs(),
        "auto-tick enabled"
    );

    task_tracker.spawn(async move {
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(5)) => {}
            _ = cancel_outer.cancelled() => {
                info!(module = "captain", "auto-tick cancelled before first run");
                return;
            }
        }

        loop {
            let mut tick_rx = tick_rx.clone();
            let mut interval = *tick_rx.borrow_and_update();
            if cancel_outer.is_cancelled() {
                info!(module = "captain", "auto-tick exiting on cancellation");
                return;
            }
            let settings = settings.clone();
            let tick_bus = tick_bus.clone();
            let tick_store = tick_store.clone();
            let cancel = cancel_outer.clone();
            let runtime = runtime.clone();

            let result = AssertUnwindSafe(async move {
                let mut consecutive_failures: u32 = 0;
                loop {
                    let expired = runtime.cleanup_expired_sessions();
                    if expired > 0 {
                        info!(
                            module = "cc-session",
                            expired,
                            "expired sessions cleaned up"
                        );
                    }

                    let cfg = settings.load_config();
                    if cfg.captain.auto_schedule {
                        let wf = settings.load_captain_workflow();
                        match crate::runtime::dashboard::trigger_captain_tick(
                            &cfg,
                            &wf,
                            false,
                            Some(&tick_bus),
                            true,
                            &tick_store,
                            &cancel,
                            runtime.task_tracker(),
                        )
                        .await
                        {
                            Ok(val) => {
                                if consecutive_failures >= degraded_failure_threshold() {
                                    tracing::info!(
                                        module = "captain",
                                        "auto-tick recovered — clearing degraded flag"
                                    );
                                }
                                consecutive_failures = 0;
                                runtime.set_health_degraded(false);
                                let workers = val.active_workers;
                                let tasks = &val.tasks;
                                info!(module = "captain", workers, tasks = ?tasks, "auto-tick completed");
                            }
                            Err(err) => {
                                consecutive_failures += 1;
                                tracing::warn!(
                                    module = "captain",
                                    error = %err,
                                    consecutive_failures,
                                    "auto-tick failed"
                                );
                                if consecutive_failures == degraded_failure_threshold() {
                                    runtime.set_health_degraded(true);
                                    tracing::error!(
                                        module = "captain",
                                        consecutive_failures,
                                        "auto-tick has failed repeatedly — marking degraded"
                                    );
                                    let payload = api_types::NotificationPayload {
                                        message: format!(
                                            "⚠️ Captain auto-tick failing ({consecutive_failures} consecutive failures). Last error: {err}"
                                        ),
                                        level: api_types::NotifyLevel::Normal,
                                        kind: api_types::NotificationKind::Generic,
                                        task_key: Some("captain:degraded".into()),
                                        reply_markup: None,
                                    };
                                    tick_bus.send(global_bus::BusPayload::Notification(payload));
                                } else if consecutive_failures > degraded_failure_threshold()
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

                    tokio::select! {
                        _ = tokio::time::sleep(interval) => {},
                        changed = tick_rx.changed() => {
                            if changed.is_err() {
                                info!(module = "captain", "auto-tick exiting: tick config channel closed");
                                return;
                            }
                            interval = *tick_rx.borrow();
                            info!(
                                module = "captain",
                                interval_s = interval.as_secs(),
                                "auto-tick interval updated"
                            );
                        },
                        _ = crate::WORKER_EXIT_SIGNAL.notified() => {
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
                    _ = tokio::time::sleep(Duration::from_secs(5)) => {}
                    _ = cancel_outer.cancelled() => {
                        info!(module = "captain", "auto-tick cancelled during panic backoff");
                        return;
                    }
                }
            }
        }
    });
}

pub(super) fn spawn_workbench_cleanup(runtime: &CaptainRuntime) {
    let pool = runtime.pool().clone();
    let cancel = runtime.cancellation_token().clone();
    runtime.task_tracker().spawn(async move {
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(300)) => {}
            _ = cancel.cancelled() => { return; }
        }
        if let Err(err) = run_workbench_cleanup(&pool).await {
            warn!(module = "cleanup", error = %err, "workbench cleanup failed");
        }
    });
}

async fn run_workbench_cleanup(pool: &sqlx::SqlitePool) -> anyhow::Result<()> {
    let stale = crate::io::queries::workbenches::stale_archived(pool, 30).await?;
    if stale.is_empty() {
        return Ok(());
    }
    info!(
        module = "cleanup",
        count = stale.len(),
        "cleaning up stale archived workbenches"
    );
    for wb in &stale {
        let wt_path = std::path::Path::new(&wb.worktree);
        if wt_path.exists() {
            let repo_path = global_git::common_repo_path(wt_path).await.ok().flatten();
            if let Some(repo) = repo_path {
                if let Err(err) = global_git::remove_worktree(&repo, wt_path).await {
                    warn!(module = "cleanup", worktree = %wb.worktree, error = %err, "git worktree remove failed");
                } else {
                    info!(module = "cleanup", worktree = %wb.worktree, "removed git worktree");
                }
            } else if let Err(err) = tokio::fs::remove_dir_all(wt_path).await {
                warn!(module = "cleanup", worktree = %wb.worktree, error = %err, "failed to remove worktree directory");
            }
        }
        let layout_path = global_types::data_dir()
            .join("workbenches")
            .join(format!("{}.json", wb.id));
        if layout_path.exists() {
            global_infra::best_effort!(
                tokio::fs::remove_file(&layout_path).await,
                "daemon_background: tokio::fs::remove_file(&layout_path).await"
            );
        }
        crate::io::queries::workbenches::mark_deleted(pool, wb.id).await?;
        info!(module = "cleanup", id = wb.id, title = %wb.title, "workbench marked deleted");
    }
    Ok(())
}

pub(super) fn spawn_credential_usage_poll(runtime: &CaptainRuntime) {
    let pool = runtime.pool().clone();
    let bus = runtime.bus().clone();
    let cancel = runtime.cancellation_token().clone();
    runtime.task_tracker().spawn(async move {
        crate::runtime::credential_usage_poll::run(pool, bus, cancel).await;
    });
}
