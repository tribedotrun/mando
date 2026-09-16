//! Rebase worker management and PR status checking — extracted from mergeability.

use crate::Task;
use anyhow::Result;

use crate::service::merge_logic;

pub(super) use super::rebase_spawn::handle_conflict;

pub(super) use global_github::MergeableStatus as MergeStatus;

/// Check PR mergeable status via the GitHub provider boundary.
#[tracing::instrument(skip_all)]
pub(super) async fn check_pr_mergeable(pr: &str, repo: &str) -> Result<MergeStatus> {
    global_github::check_pr_mergeable(pr, repo).await
}

/// Reap dead rebase workers and detect success via SHA comparison.
///
/// Uses pid_registry for PID lookup.
#[tracing::instrument(skip_all)]
pub(super) async fn reap_dead_rebase_workers(items: &mut [Task], pool: &sqlx::SqlitePool) {
    for item in items.iter_mut() {
        let rw = match &item.rebase_worker {
            Some(rw) if rw != "failed" => rw.clone(),
            _ => continue,
        };
        // Look up rebase worker PID from pid_registry (registered by session_name).
        let pid = crate::io::pid_registry::get_pid(&rw).unwrap_or(crate::Pid::new(0));
        if pid.as_u32() != 0 && global_claude::is_process_alive(pid) {
            continue; // still running
        }

        // Worker exited. Check if it succeeded by comparing HEAD SHA.
        let wt = item.worktree.as_deref().unwrap_or("");
        let succeeded = if !wt.is_empty() {
            let wt_path = global_infra::paths::expand_tilde(wt);
            match global_git::head_sha_short(&wt_path).await {
                Ok(current_sha) => {
                    merge_logic::did_rebase_succeed(item.rebase_head_sha.as_deref(), &current_sha)
                }
                Err(e) => {
                    tracing::warn!(
                        module = "captain",
                        worker = %rw,
                        wt = %wt,
                        error = %e,
                        "failed to read HEAD SHA for rebase success detection, treating as failure"
                    );
                    false
                }
            }
        } else {
            false
        };

        // Log success/failure but do NOT mutate item fields yet —
        // rebase_head_sha must be preserved for correct re-evaluation
        // if finalization is retried on the next tick.
        if succeeded {
            tracing::info!(
                module = "captain",
                worker = %rw,
                "rebase worker succeeded (SHA changed)"
            );
        } else {
            tracing::info!(
                module = "captain",
                worker = %rw,
                retries = item.rebase_retries,
                "rebase worker failed (SHA unchanged)"
            );
        }

        // Mark session as completed/failed in the DB, and collect session_id
        // for PID unregistration.
        let status = if succeeded {
            global_types::SessionStatus::Stopped
        } else {
            global_types::SessionStatus::Failed
        };
        let mut session_finalized = true;
        let found_sid = match sessions_db::find_session_id_by_worker_name(pool, &rw).await {
            Ok(Some(sid)) => {
                // Check stream state to decide whether to finalize now or
                // retry next tick. Only retry when the result event hasn't
                // been written yet AND the stream file exists (CC is still
                // buffering). Finalize immediately when:
                //  - stream file missing (CC crashed before creating it)
                //  - result event present but duration_ms absent (write done)
                //  - result event present with duration_ms (happy path)
                let stream_path = global_infra::paths::stream_path_for_session(&sid);
                let cost_info = global_claude::get_stream_cost(&stream_path);
                let should_retry = cost_info.is_none() && stream_path.exists();

                if !should_retry {
                    let cwd = wt.to_string();
                    if let Err(e) = crate::io::headless_cc::log_session_completion(
                        pool,
                        &sid,
                        &cwd,
                        "rebase",
                        &rw,
                        Some(item.id),
                        status,
                    )
                    .await
                    {
                        tracing::warn!(
                            module = "captain",
                            session_id = %sid,
                            error = %e,
                            "failed to log rebase session completion"
                        );
                    }
                    if !succeeded {
                        // Codex app-server turns finalize themselves before
                        // this SHA-based reaper can classify the rebase
                        // outcome. Preserve recorded cost/duration, but
                        // downgrade the row to Failed when HEAD did not move.
                        if let Err(e) = sessions_db::update_session_status(
                            pool,
                            &sid,
                            global_types::SessionStatus::Failed,
                        )
                        .await
                        {
                            tracing::warn!(
                                module = "captain",
                                session_id = %sid,
                                error = %e,
                                "failed to mark unsuccessful rebase session failed"
                            );
                        }
                    }
                } else {
                    tracing::info!(
                        module = "captain",
                        session_id = %sid,
                        worker = %rw,
                        "rebase session stream has no result event yet — retrying next tick"
                    );
                    session_finalized = false;
                }
                Some(sid)
            }
            Ok(None) => {
                tracing::debug!(
                    module = "captain",
                    worker = %rw,
                    "no running session found for rebase worker — skipping completion log"
                );
                None
            }
            Err(e) => {
                tracing::warn!(
                    module = "captain",
                    worker = %rw,
                    error = %e,
                    "failed to look up rebase session by worker_name"
                );
                None
            }
        };

        if session_finalized {
            // Apply rebase outcome mutations only after finalization is
            // confirmed. If retried, rebase_head_sha must be intact for
            // correct re-evaluation of did_rebase_succeed().
            if succeeded {
                item.rebase_retries = 0;
                item.rebase_head_sha = None;
            }
            // Rebase lifecycle done: unregister both PIDs and clear worker.
            if let Err(e) = crate::io::pid_registry::unregister(&rw) {
                tracing::warn!(module = "captain", worker = %rw, %e, "pid_registry unregister failed on rebase completion");
            }
            if let Some(ref sid) = found_sid {
                global_infra::best_effort!(
                    crate::io::pid_registry::unregister(sid),
                    "mergeability_rebase: crate::io::pid_registry::unregister(sid)"
                );
            }
            item.rebase_worker = None;
        } else {
            // Stream not yet flushed: keep rebase_worker set so the reaper
            // retries next tick (prevents duplicate spawn via
            // items_needing_rebase_check). Unregister session_id PID so the
            // same-tick reconciler L1 doesn't terminate with wrong status.
            if let Some(ref sid) = found_sid {
                global_infra::best_effort!(
                    crate::io::pid_registry::unregister(sid),
                    "mergeability_rebase: crate::io::pid_registry::unregister(sid)"
                );
            }
        }
    }
}
