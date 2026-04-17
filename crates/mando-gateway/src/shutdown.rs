//! Graceful shutdown helpers.
//!
//! Signals every Claude Code subprocess recorded in the PID registry
//! before tokio cancellation tears down the read loops. Without this,
//! subprocesses become orphans whose stdout nobody reads, and their
//! final `result` event is silently dropped. Pairs with
//! `pid_registry::cleanup_on_startup`, which handles anything that
//! does survive past the grace window.

use tracing::{info, warn};

/// Total wall clock this helper is allowed to spend. One `kill_process`
/// call can already take up to 5s internally for the SIGTERM grace
/// window before escalating to SIGKILL; the budget here bounds the
/// aggregate across all subprocesses.
const SHUTDOWN_GRACE: std::time::Duration = std::time::Duration::from_secs(5);

/// Signal every live CC subprocess and wait (bounded by
/// `SHUTDOWN_GRACE`) for them to exit, then emit a terminal log line so
/// operators can tell what state the daemon exited in.
pub async fn signal_cc_subprocesses_for_shutdown() {
    let map = match captain::io::pid_registry::snapshot() {
        Ok(m) => m,
        Err(e) => {
            warn!(module = "shutdown", error = %e, "could not snapshot pid registry for shutdown");
            return;
        }
    };
    if map.is_empty() {
        return;
    }
    // The fingerprint stored with each entry is only useful for the
    // cross-restart PID-reuse check; at shutdown we're signalling
    // subprocesses we just spawned, so the bare PID is enough.
    let pids: Vec<_> = map.into_values().map(|entry| entry.pid).collect();
    let total = pids.len();
    info!(
        module = "shutdown",
        total, "signalling CC subprocesses before exit"
    );

    // Parallel SIGTERM+reap with a single overall deadline so one slow
    // subprocess can't drag shutdown past the grace window.
    let kills = pids.into_iter().map(|pid| async move {
        let _ = global_claude::kill_process(pid).await;
        !global_claude::is_process_alive(pid)
    });
    let joined = futures_util::future::join_all(kills);
    match tokio::time::timeout(SHUTDOWN_GRACE, joined).await {
        Ok(results) => {
            let confirmed = results.iter().filter(|d| **d).count();
            let still_alive = total - confirmed;
            if still_alive > 0 {
                warn!(
                    module = "shutdown",
                    total,
                    confirmed,
                    still_alive,
                    "some CC subprocesses survived SIGTERM at exit; orphans will be cleaned up on next daemon startup"
                );
            } else {
                info!(
                    module = "shutdown",
                    total, "all CC subprocesses signalled and confirmed exited"
                );
            }
        }
        Err(_) => {
            warn!(
                module = "shutdown",
                total,
                grace_s = SHUTDOWN_GRACE.as_secs(),
                "shutdown grace window expired before CC subprocesses confirmed exit; orphans will be cleaned up on next daemon startup"
            );
        }
    }
}
