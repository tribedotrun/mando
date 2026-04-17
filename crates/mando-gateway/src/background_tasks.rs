//! Background task spawners — captain auto-tick loop + workbench cleanup.

use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use futures_util::FutureExt;
use tokio::sync::watch;
use tracing::info;

use sqlx::SqlitePool;
use tracing::warn;

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
pub fn spawn_auto_tick(state: &AppState, tick_rx: watch::Receiver<Duration>) {
    let tick_config = state.config.clone();
    let tick_workflow = state.captain_workflow.clone();
    let tick_bus = state.bus.clone();
    let tick_store = state.task_store.clone();
    let tick_sessions = state.cc_session_mgr.clone();
    let cancel_outer = state.cancellation_token.clone();
    let initial_interval = *tick_rx.borrow();

    info!(
        module = "captain",
        interval_s = initial_interval.as_secs(),
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
            let mut tick_rx = tick_rx.clone();
            let mut interval = *tick_rx.borrow_and_update();
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
                        match captain::runtime::dashboard::trigger_captain_tick(
                            &cfg,
                            &wf,
                            false,
                            Some(&tick_bus),
                            true,
                            &tick_store,
                            &cancel,
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
                                    let payload = global_types::events::NotificationPayload {
                                        message: format!(
                                            "\u{26a0}\u{fe0f} Captain auto-tick failing ({consecutive_failures} consecutive failures). Last error: {e}"
                                        ),
                                        level: global_types::NotifyLevel::Normal,
                                        kind: global_types::events::NotificationKind::Generic,
                                        task_key: Some("captain:degraded".into()),
                                        reply_markup: None,
                                    };
                                    match serde_json::to_value(&payload) {
                                        Ok(json) => tick_bus.send(
                                            global_types::BusEvent::Notification,
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
                        _ = captain::WORKER_EXIT_SIGNAL.notified() => {
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

/// Spawn terminal auto-resume: waits a few seconds for the server to be
/// fully ready, then re-creates terminals that were alive when the daemon
/// last exited. Each is re-spawned with `claude --resume` (bare, no
/// session ID) so Claude Code picks up the most recent session in that CWD.
pub fn spawn_terminal_auto_resume(state: &AppState) {
    let terminal_host = state.terminal_host.clone();
    let config = state.config.clone();
    let listen_port = state.listen_port;
    let cancel = state.cancellation_token.clone();

    state.task_tracker.spawn(async move {
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(3)) => {}
            _ = cancel.cancelled() => { return; }
        }

        let restorable = terminal_host.take_restorable();
        if restorable.is_empty() {
            return;
        }

        let total = restorable.len();
        info!(
            module = "terminal",
            count = total,
            "auto-resuming terminal sessions"
        );

        let mut resumed = 0usize;
        let mut skipped = 0usize;
        for old in restorable {
            if cancel.is_cancelled() {
                info!(module = "terminal", "auto-resume cancelled");
                return;
            }
            if old.cc_session_id.is_none() {
                info!(
                    module = "terminal",
                    session = old.id,
                    "skipping auto-resume — no CC session ID captured"
                );
                skipped += 1;
                continue;
            }
            if !old.cwd.is_dir() {
                info!(
                    module = "terminal",
                    session = old.id,
                    cwd = %old.cwd.display(),
                    "skipping auto-resume — cwd no longer exists"
                );
                skipped += 1;
                continue;
            }

            let cfg = config.load();
            let args_str = match &old.agent {
                terminal::Agent::Claude => cfg.captain.claude_terminal_args.clone(),
                terminal::Agent::Codex => cfg.captain.codex_terminal_args.clone(),
            };
            let config_env = cfg.env.clone();
            drop(cfg);

            let extra_args = match shell_words::split(&args_str) {
                Ok(args) => args,
                Err(e) => {
                    warn!(
                        module = "terminal",
                        session = old.id,
                        error = %e,
                        "failed to parse terminal args for auto-resume"
                    );
                    skipped += 1;
                    continue;
                }
            };

            let mut terminal_env = std::collections::HashMap::new();
            terminal_env.insert("MANDO_PORT".to_string(), listen_port.to_string());
            let auth_token = transport_http::auth::ensure_auth_token();
            terminal_env.insert("MANDO_AUTH_TOKEN".to_string(), auth_token);

            let req = terminal::CreateRequest {
                project: old.project.clone(),
                cwd: old.cwd.clone(),
                agent: old.agent.clone(),
                resume_session_id: old.cc_session_id.clone(),
                size: Some(old.size),
                config_env,
                terminal_env,
                terminal_id: old.terminal_id.clone(),
                extra_args,
                name: old.name.clone(),
            };

            match terminal_host.create(req) {
                Ok(session) => {
                    terminal_host.delete_restored_history(&old.id);
                    resumed += 1;
                    info!(
                        module = "terminal",
                        old_id = old.id,
                        new_id = session.info().id,
                        project = old.project,
                        "auto-resumed terminal session"
                    );
                }
                Err(e) => {
                    warn!(
                        module = "terminal",
                        session = old.id,
                        error = %e,
                        "failed to auto-resume terminal session"
                    );
                }
            }
        }

        info!(
            module = "terminal",
            total,
            resumed,
            skipped,
            failed = total - resumed - skipped,
            "terminal auto-resume complete"
        );
    });
}

/// Spawn the workbench cleanup job: waits 5 minutes after startup, then
/// removes worktree directories and layout JSONs for workbenches that have
/// been archived for more than 30 days.  DB rows are kept (deleted_at set)
/// as audit trail.
pub fn spawn_workbench_cleanup(state: &AppState) {
    let pool = state.db.pool().clone();
    let cancel = state.cancellation_token.clone();
    state.task_tracker.spawn(async move {
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_secs(300)) => {}
            _ = cancel.cancelled() => { return; }
        }
        if let Err(e) = run_workbench_cleanup(&pool).await {
            warn!(module = "cleanup", error = %e, "workbench cleanup failed");
        }
    });
}

// ── Workbench cleanup ───────────────────────────────────────────────

async fn run_workbench_cleanup(pool: &SqlitePool) -> anyhow::Result<()> {
    let stale = captain::io::queries::workbenches::stale_archived(pool, 30).await?;
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
            // Resolve the repo path by reading the .git file inside the worktree
            // (it contains "gitdir: <repo>/.git/worktrees/<name>").
            let repo_result = tokio::process::Command::new("git")
                .args(["rev-parse", "--git-common-dir"])
                .current_dir(wt_path)
                .output()
                .await;
            let repo_path = repo_result.ok().and_then(|o| {
                if o.status.success() {
                    let git_dir = String::from_utf8_lossy(&o.stdout).trim().to_string();
                    std::path::Path::new(&git_dir)
                        .parent()
                        .map(|p| p.to_path_buf())
                } else {
                    None
                }
            });
            if let Some(repo) = repo_path {
                if let Err(e) = captain::io::git::remove_worktree(&repo, wt_path).await {
                    warn!(module = "cleanup", worktree = %wb.worktree, error = %e, "git worktree remove failed");
                } else {
                    info!(module = "cleanup", worktree = %wb.worktree, "removed git worktree");
                }
            } else if let Err(e) = tokio::fs::remove_dir_all(wt_path).await {
                warn!(module = "cleanup", worktree = %wb.worktree, error = %e, "failed to remove worktree directory");
            }
        }
        let layout_path = global_types::data_dir()
            .join("workbenches")
            .join(format!("{}.json", wb.id));
        if layout_path.exists() {
            let _ = tokio::fs::remove_file(&layout_path).await;
        }
        captain::io::queries::workbenches::mark_deleted(pool, wb.id).await?;
        info!(module = "cleanup", id = wb.id, title = %wb.title, "workbench marked deleted");
    }
    Ok(())
}
