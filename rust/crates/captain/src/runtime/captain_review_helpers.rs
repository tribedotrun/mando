//! Shared helpers for captain review verdict application.

use tracing::warn;

use crate::Task;
use settings::CaptainWorkflow;

use sqlx::SqlitePool;

pub(crate) fn escaped_title(item: &Task) -> String {
    global_infra::html::escape_html(&item.title)
}

/// Inline resume of a worker process with feedback. Shared by `nudge` and
/// `reset_budget` verdict handlers. Kills old process, checks for broken
/// stream, resumes with feedback, updates health state and session log.
///
/// Returns `true` if the worker was successfully resumed.
#[tracing::instrument(skip_all)]
pub(super) async fn inline_resume_worker(
    item: &Task,
    feedback: &str,
    workflow: &CaptainWorkflow,
    pool: &SqlitePool,
) -> bool {
    let (Some(w), Some(sid), Some(wt)) = (&item.worker, &item.session_ids.worker, &item.worktree)
    else {
        warn!(
            module = "captain",
            item_id = item.id,
            "verdict resume has no worker/session/worktree; next tick will handle"
        );
        return false;
    };

    let wt_path = global_infra::paths::expand_tilde(wt);
    let current_pid = crate::io::pid_lookup::resolve_pid(sid, w).unwrap_or(crate::Pid::new(0));
    let delivery = match super::agent_runtime::nudge_worker(
        pool,
        item,
        w,
        &wt_path,
        feedback,
        sid,
        &workflow.models.worker,
        workflow,
        current_pid,
    )
    .await
    {
        Ok(super::agent_runtime::AgentNudgeOutcome::Delivered(delivery)) => delivery,
        Ok(super::agent_runtime::AgentNudgeOutcome::BrokenSession { alert }) => {
            warn!(module = "captain", worker = %w, %sid, %alert, "verdict skipped resume; stream is broken");
            return false;
        }
        Err(e) => {
            warn!(module = "captain", worker = %w, error = %e,
                "verdict resume failed; next tick will retry");
            return false;
        }
    };
    {
        // Health-state bookkeeping must not abort: the worker is already
        // running. Degrade gracefully on failure instead of double-resuming.
        let health_path = crate::config::worker_health_path();
        match crate::io::health_store::load_health_state(&health_path) {
            Ok(mut hstate) => {
                crate::io::health_store::set_health_field(
                    &mut hstate,
                    w,
                    "pid",
                    serde_json::json!(delivery.pid),
                );
                crate::io::health_store::set_health_field(
                    &mut hstate,
                    w,
                    "stream_size_at_spawn",
                    serde_json::json!(delivery.stream_size_before),
                );
                if let Err(e) = crate::io::health_store::save_health_state(&health_path, &hstate) {
                    warn!(module = "captain", worker = %w, error = %e,
                            "failed to persist health after verdict resume");
                }
            }
            Err(e) => {
                warn!(module = "captain", worker = %w, error = %e,
                        "failed to load health state after verdict resume; skipping bookkeeping");
            }
        }
        true
    }
}
