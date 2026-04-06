//! mando-captain — captain tick loop, task engine, worker management,
//! and the deterministic state machine.
//!
//! Layer discipline:
//! - `biz/`     — pure functions, no I/O
//! - `io/`      — thin async wrappers around external systems (pub(crate))
//! - `runtime/` — orchestration: composes biz + io

// All io/ functions are now wired (issue #4).

pub mod biz;
pub mod io;
pub(crate) mod pr_evidence;
pub mod runtime;

/// Signal that a worker process has exited. The gateway auto-tick loop listens
/// on this to trigger an immediate captain tick instead of waiting for the next
/// scheduled interval.
pub static WORKER_EXIT_SIGNAL: tokio::sync::Notify = tokio::sync::Notify::const_new();

/// Spawn a background task that awaits the child process and signals
/// [`WORKER_EXIT_SIGNAL`] on exit so the next tick fires immediately.
///
/// TRACKED: not registered with the gateway's TaskTracker because mando-captain
/// is a library crate and has no dependency on the gateway's AppState. The child
/// process itself owns its lifecycle and is separately killed on gateway shutdown
/// via the pid registry; this watcher only observes exit.
pub fn watch_worker_exit(mut child: tokio::process::Child) {
    tokio::spawn(async move {
        match child.wait().await {
            Ok(status) => {
                tracing::debug!(
                    module = "captain",
                    exit_code = status.code(),
                    "worker process exited"
                );
            }
            Err(e) => {
                tracing::warn!(
                    module = "captain",
                    error = %e,
                    "worker process wait failed — signaling exit anyway"
                );
            }
        }
        // notify_one stores a permit when no waiter is present, so the
        // next select! iteration picks it up immediately. Multiple exits
        // coalesce into one permit, which is fine — one tick evaluates all
        // workers. (notify_waiters would silently drop the signal when the
        // tick loop is busy processing, which is the common case.)
        WORKER_EXIT_SIGNAL.notify_one();
    });
}
