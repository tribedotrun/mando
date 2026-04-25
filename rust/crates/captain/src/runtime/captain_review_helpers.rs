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

    let stream_path = global_infra::paths::stream_path_for_session(sid);
    if global_claude::stream_has_broken_session(&stream_path) {
        warn!(
            module = "captain", worker = %w,
            "verdict skipped resume; stream is broken, next tick will handle"
        );
        return false;
    }
    let symptoms = global_claude::StreamSymptomMatcher::new(workflow.stream_symptoms.clone());
    if let Some(m) = global_claude::stream_broken_session_symptom(&stream_path, &symptoms) {
        warn!(
            module = "captain",
            worker = %w,
            symptom = %m.reason,
            origin = %m.origin.tag(),
            "verdict skipped resume; stream already carries a broken-session symptom"
        );
        return false;
    }

    let old_pid = crate::io::pid_lookup::resolve_pid(sid, w).unwrap_or(crate::Pid::new(0));
    if old_pid.as_u32() > 0 {
        if let Err(e) = global_claude::kill_process(old_pid).await {
            warn!(
                module = "captain", worker = %w, pid = %old_pid, error = %e,
                "failed to kill old process before verdict resume"
            );
        }
    }

    let wt_path = global_infra::paths::expand_tilde(wt);
    let stream_size_before = global_claude::get_stream_file_size(&stream_path);
    let (env, cred_id) = super::spawner::credential_env_for_session(pool, sid).await;
    match crate::io::process_manager::resume_worker_process(
        feedback,
        &wt_path,
        &workflow.models.worker,
        sid,
        &env,
    )
    .await
    {
        Ok((pid, _)) => {
            if let Err(e) = crate::io::pid_registry::register(sid, pid) {
                warn!(module = "captain", worker = %w, %e, "pid_registry register failed");
            }
            // Health-state bookkeeping must not abort: the worker is already
            // running. Degrade gracefully on failure instead of double-resuming.
            let health_path = crate::config::worker_health_path();
            match crate::io::health_store::load_health_state(&health_path) {
                Ok(mut hstate) => {
                    crate::io::health_store::set_health_field(
                        &mut hstate,
                        w,
                        "pid",
                        serde_json::json!(pid),
                    );
                    crate::io::health_store::set_health_field(
                        &mut hstate,
                        w,
                        "stream_size_at_spawn",
                        serde_json::json!(stream_size_before),
                    );
                    if let Err(e) =
                        crate::io::health_store::save_health_state(&health_path, &hstate)
                    {
                        warn!(module = "captain", worker = %w, error = %e,
                            "failed to persist health after verdict resume");
                    }
                }
                Err(e) => {
                    warn!(module = "captain", worker = %w, error = %e,
                        "failed to load health state after verdict resume; skipping bookkeeping");
                }
            }
            if let Err(e) = crate::io::headless_cc::log_running_session(
                pool,
                sid,
                &wt_path,
                "worker",
                w,
                Some(item.id),
                true,
                cred_id,
            )
            .await
            {
                warn!(module = "captain", worker = %w, %e,
                    "failed to log running session after verdict resume");
            }
            true
        }
        Err(e) => {
            warn!(module = "captain", worker = %w, error = %e,
                "verdict resume failed; next tick will retry");
            false
        }
    }
}
