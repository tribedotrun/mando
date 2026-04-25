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

#[cfg(test)]
mod tests {
    use super::*;

    /// Set MANDO_DATA_DIR to a temp directory for isolation, returning the
    /// path together with a guard that restores the previous env value.
    fn isolate_data_dir() -> (std::path::PathBuf, global_infra::EnvVarGuard) {
        let dir = std::env::temp_dir().join(format!(
            "mando-rebase-test-{}",
            global_infra::uuid::Uuid::v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let guard = global_infra::EnvVarGuard::set("MANDO_DATA_DIR", &dir);
        (dir, guard)
    }

    async fn test_pool() -> sqlx::SqlitePool {
        let db = global_db::Db::open_in_memory().await.unwrap();
        db.pool().clone()
    }

    #[tokio::test]
    async fn reap_defers_when_stream_cost_missing() {
        let _lock = global_infra::PROCESS_ENV_LOCK.lock().await;
        let (data_dir, _guard) = isolate_data_dir();
        let pool = test_pool().await;

        // Create a session_id and worker_name.
        let session_id = global_infra::uuid::Uuid::v4().to_string();
        let worker_name = "mando-rebase-42";

        // Insert a "running" session in the DB.
        crate::io::headless_cc::log_running_session(
            &pool,
            &session_id,
            std::path::Path::new("/tmp"),
            "rebase",
            worker_name,
            Some(99),
            false,
            None,
        )
        .await
        .unwrap();

        // Register a dead PID for the worker name (PID 999999 is not alive).
        crate::io::pid_registry::register(worker_name, crate::Pid::new(999999)).unwrap();
        crate::io::pid_registry::register(&session_id, crate::Pid::new(999999)).unwrap();

        // Create a stream file WITHOUT duration_ms (no result event at all).
        let streams_dir = data_dir.join("state/cc-streams");
        std::fs::create_dir_all(&streams_dir).unwrap();
        let stream_file = streams_dir.join(format!("{session_id}.jsonl"));
        std::fs::write(&stream_file, r#"{"type":"system","subtype":"init"}"#).unwrap();

        // Build a task with rebase_worker set.
        let mut task = Task::new("Test rebase defer");
        task.id = 99;
        task.rebase_worker = Some(worker_name.to_string());
        let mut items = vec![task];

        // First reap: stream has no result event, should DEFER.
        reap_dead_rebase_workers(&mut items, &pool).await;

        // rebase_worker should NOT be cleared (deferred).
        assert!(
            items[0].rebase_worker.is_some(),
            "rebase_worker should stay set when stream cost is missing"
        );

        // Session should still be "running" in the DB.
        let sid = sessions_db::find_session_id_by_worker_name(&pool, worker_name)
            .await
            .unwrap();
        assert_eq!(
            sid.as_deref(),
            Some(session_id.as_str()),
            "session should still be findable as running"
        );

        // session_id PID should be unregistered (blocks same-tick reconciler).
        assert!(
            crate::io::pid_registry::get_pid(&session_id).is_none(),
            "session_id PID should be unregistered to block reconciler L1"
        );

        // Now add a result event WITH duration_ms to the stream file.
        std::fs::write(
            &stream_file,
            [
                r#"{"type":"system","subtype":"init"}"#,
                r#"{"type":"result","subtype":"success","total_cost_usd":0.02,"duration_ms":5000}"#,
            ]
            .join("\n"),
        )
        .unwrap();

        // Re-register worker PID (reaper needs it for the retry).
        // worker_name PID was kept from first pass; re-register session_id for
        // the finalize path (log_session_completion reads cost via session_id).
        // In production, the session_id PID would already be unregistered;
        // finalization works because log_session_completion uses the session_id
        // string directly, not the PID registry.

        // Second reap: stream now has duration_ms, should FINALIZE.
        reap_dead_rebase_workers(&mut items, &pool).await;

        // rebase_worker should be cleared (finalized).
        assert!(
            items[0].rebase_worker.is_none(),
            "rebase_worker should be cleared after finalization"
        );

        // Check DB for duration_ms and correct status.
        let row = sessions_db::session_by_id(&pool, &session_id)
            .await
            .unwrap()
            .expect("session should exist");
        assert_eq!(
            row.duration_ms,
            Some(5000),
            "duration_ms should be persisted after finalization"
        );
        // No worktree set -> SHA check returns false -> status should be Failed.
        assert_eq!(
            row.status, "failed",
            "status should reflect SHA-based outcome"
        );

        // Cleanup.
        global_infra::best_effort!(
            std::fs::remove_dir_all(&data_dir),
            "mergeability_rebase: std::fs::remove_dir_all(&data_dir)"
        );
    }

    /// Verify that rebase_head_sha is preserved across retry ticks so a
    /// successful rebase is not misclassified as failed on the second pass.
    #[tokio::test]
    async fn reap_preserves_head_sha_across_retry() {
        let _lock = global_infra::PROCESS_ENV_LOCK.lock().await;
        let (data_dir, _guard) = isolate_data_dir();
        let pool = test_pool().await;

        let session_id = global_infra::uuid::Uuid::v4().to_string();
        let worker_name = "mando-rebase-77";

        crate::io::headless_cc::log_running_session(
            &pool,
            &session_id,
            std::path::Path::new("/tmp"),
            "rebase",
            worker_name,
            Some(88),
            false,
            None,
        )
        .await
        .unwrap();

        crate::io::pid_registry::register(worker_name, crate::Pid::new(999999)).unwrap();
        crate::io::pid_registry::register(&session_id, crate::Pid::new(999999)).unwrap();

        let streams_dir = data_dir.join("state/cc-streams");
        std::fs::create_dir_all(&streams_dir).unwrap();
        let stream_file = streams_dir.join(format!("{session_id}.jsonl"));
        // No result event -> retry path.
        std::fs::write(&stream_file, r#"{"type":"system","subtype":"init"}"#).unwrap();

        let mut task = Task::new("Test SHA preservation");
        task.id = 88;
        task.rebase_worker = Some(worker_name.to_string());
        // Set a known SHA that matches nothing (simulates pre-rebase baseline).
        task.rebase_head_sha = Some("abc123".to_string());

        let mut items = vec![task];

        // First reap: no stream result -> retry.
        reap_dead_rebase_workers(&mut items, &pool).await;

        // rebase_head_sha must be preserved for correct re-evaluation.
        assert_eq!(
            items[0].rebase_head_sha.as_deref(),
            Some("abc123"),
            "rebase_head_sha must survive the retry so did_rebase_succeed works correctly next tick"
        );

        // Cleanup.
        global_infra::best_effort!(
            std::fs::remove_dir_all(&data_dir),
            "mergeability_rebase: std::fs::remove_dir_all(&data_dir)"
        );
    }
}
