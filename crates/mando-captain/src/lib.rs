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
pub fn watch_worker_exit(
    mut child: tokio::process::Child,
    pid: mando_types::Pid,
    session_id: &str,
) {
    let session_id = session_id.to_string();
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
        // Best-effort cleanup on natural exit. Detached helper processes started
        // by the worker may still be attached to the worker's process group even
        // after the Claude parent exits. If this session still owns the same PID
        // in the registry, try to terminate the process group and then clear the
        // registry entry. This is idempotent with explicit cancel/reopen cleanup.
        if crate::io::pid_registry::get_pid(&session_id) == Some(pid) {
            if let Err(e) = mando_cc::kill_process(pid).await {
                tracing::warn!(
                    module = "captain",
                    %session_id,
                    %pid,
                    error = %e,
                    "worker-exit cleanup kill failed"
                );
            }
            if let Err(e) = crate::io::pid_registry::unregister(&session_id) {
                tracing::warn!(
                    module = "captain",
                    %session_id,
                    error = %e,
                    "worker-exit cleanup unregister failed"
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
