//! Session status reconciliation — safety net for stale "running" sessions.
//!
//! Provider-aware detection on each tick:
//! L1: Terminal stream result — current turn/session finished, settle DB status
//! L2: Provider liveness — Claude uses its per-session process; Codex uses active turn state
//! L3: Inactive stale state — stale/missing Codex turns fail, stale/dead Claude sessions stop
//!
//! All detection is synchronous; terminations run in parallel via `join_all`.

use global_types::SessionStatus;
use sqlx::SqlitePool;
use std::path::Path;

enum StreamFreshness {
    Present(f64),
    Missing,
    MetadataError(String),
}

fn stream_freshness(stream_path: &Path) -> StreamFreshness {
    let metadata = match std::fs::metadata(stream_path) {
        Ok(metadata) => metadata,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return StreamFreshness::Missing,
        Err(e) => {
            return StreamFreshness::MetadataError(format!("failed to read stream metadata: {e}"));
        }
    };
    let modified = match metadata.modified() {
        Ok(modified) => modified,
        Err(e) => {
            return StreamFreshness::MetadataError(format!("failed to read stream mtime: {e}"))
        }
    };
    match modified.elapsed() {
        Ok(elapsed) => StreamFreshness::Present(elapsed.as_secs_f64()),
        Err(e) => StreamFreshness::MetadataError(format!("stream mtime is in the future: {e}")),
    }
}

/// Reconcile all sessions currently marked "running" against PID + stream truth.
///
/// Reconcile failures (DB query error) are pushed into `alerts` so the tick
/// surfaces them alongside other problems instead of burying them in logs.
#[tracing::instrument(skip_all)]
pub(crate) async fn reconcile_running_sessions(
    pool: &SqlitePool,
    stale_threshold: std::time::Duration,
    alerts: &mut Vec<String>,
) {
    let stale_threshold_s = stale_threshold.as_secs_f64();
    let running = match sessions_db::list_running_sessions(pool).await {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!(module = "captain", error = %e, "session reconciliation: failed to query");
            alerts.push(format!("session reconciliation query failed: {e}"));
            return;
        }
    };

    if running.is_empty() {
        return;
    }

    // Phase 1: Determine which sessions need termination (all checks are sync).
    struct TermJob {
        session_id: String,
        provider: global_types::TaskProvider,
        status: SessionStatus,
        reason: &'static str,
    }
    let mut jobs: Vec<TermJob> = Vec::new();

    for row in &running {
        let sid = &row.session_id;
        let stream_path = super::agent_runtime::stream_path(row.provider, sid);
        let pid = crate::io::pid_registry::get_verified_pid(sid).unwrap_or(crate::Pid::new(0));
        let liveness =
            super::agent_runtime::session_liveness(row.provider, sid, pid, &stream_path).await;

        match liveness {
            super::agent_runtime::AgentLivenessStatus::Completed => {
                jobs.push(TermJob {
                    session_id: sid.clone(),
                    provider: row.provider,
                    status: SessionStatus::Stopped,
                    reason: "terminal_clean_result",
                });
                continue;
            }
            super::agent_runtime::AgentLivenessStatus::Failed => {
                if row.provider == global_types::TaskProvider::Claude
                    && super::credential_rate_limit::check_and_activate_from_stream(pool, sid).await
                {
                    tracing::info!(
                        module = "captain",
                        session_id = %sid,
                        "session reconciliation detected rate limit — cooldown activated"
                    );
                }
                jobs.push(TermJob {
                    session_id: sid.clone(),
                    provider: row.provider,
                    status: SessionStatus::Failed,
                    reason: "terminal_error_result",
                });
                continue;
            }
            super::agent_runtime::AgentLivenessStatus::Active => {
                // Live Claude rate-limit detection kills only Claude's per-session
                // process. Codex uses a daemon-scoped shared app-server, so a
                // live Codex turn must finish through turn/completed or timeout
                // handling rather than process-level cleanup.
                if row.provider == global_types::TaskProvider::Claude
                    && global_claude::has_rate_limit_rejection(&stream_path).is_some()
                    && super::credential_rate_limit::check_and_activate_from_stream(pool, sid).await
                {
                    tracing::warn!(
                        module = "captain",
                        session_id = %sid,
                        ?pid,
                        "live session hit rate limit — killing process so tick reopens with healthy credential"
                    );
                    if let Err(e) = global_claude::kill_process(pid).await {
                        tracing::warn!(
                            module = "captain",
                            session_id = %sid,
                            error = %e,
                            "failed to kill rate-limited process"
                        );
                    }
                    jobs.push(TermJob {
                        session_id: sid.clone(),
                        provider: row.provider,
                        status: SessionStatus::Failed,
                        reason: "live_claude_rate_limit",
                    });
                }
                continue;
            }
            super::agent_runtime::AgentLivenessStatus::Inactive => {}
        }

        match row.provider {
            global_types::TaskProvider::Codex => match stream_freshness(&stream_path) {
                StreamFreshness::Missing => {
                    tracing::info!(
                        module = "captain",
                        session_id = %sid,
                        provider = %row.provider.as_str(),
                        stream_path = %stream_path.display(),
                        reason = "codex_missing_stream",
                        "session reconciliation failing inactive Codex session"
                    );
                    jobs.push(TermJob {
                        session_id: sid.clone(),
                        provider: row.provider,
                        status: SessionStatus::Failed,
                        reason: "codex_missing_stream",
                    });
                }
                StreamFreshness::Present(stale_secs) if stale_secs > stale_threshold_s => {
                    tracing::info!(
                        module = "captain",
                        session_id = %sid,
                        provider = %row.provider.as_str(),
                        stream_path = %stream_path.display(),
                        stale_secs,
                        reason = "codex_inactive_stale_stream",
                        "session reconciliation failing inactive Codex session"
                    );
                    jobs.push(TermJob {
                        session_id: sid.clone(),
                        provider: row.provider,
                        status: SessionStatus::Failed,
                        reason: "codex_inactive_stale_stream",
                    });
                }
                StreamFreshness::Present(_) => {}
                StreamFreshness::MetadataError(message) => {
                    tracing::warn!(
                        module = "captain",
                        session_id = %sid,
                        provider = %row.provider.as_str(),
                        stream_path = %stream_path.display(),
                        error = %message,
                        "session reconciliation skipped inactive Codex session because stream freshness is unknown"
                    );
                    alerts.push(format!(
                            "session reconciliation stream freshness failed for {sid} (provider={}, path={}): {message}",
                            row.provider.as_str(),
                            stream_path.display()
                        ));
                }
            },
            global_types::TaskProvider::Claude => {
                if pid.as_u32() > 0 {
                    jobs.push(TermJob {
                        session_id: sid.clone(),
                        provider: row.provider,
                        status: SessionStatus::Stopped,
                        reason: "dead_claude_pid",
                    });
                    continue;
                }
                if let Some(stale_secs) = global_claude::stream_stale_seconds(&stream_path) {
                    if stale_secs > stale_threshold_s {
                        jobs.push(TermJob {
                            session_id: sid.clone(),
                            provider: row.provider,
                            status: SessionStatus::Stopped,
                            reason: "stale_claude_stream",
                        });
                    }
                }
            }
        }
    }

    if jobs.is_empty() {
        return;
    }

    // Log rate-limit status for every terminated session (covers workers
    // which don't go through the Notifier's check_rate_limit path).
    for job in &jobs {
        let stream_path = super::agent_runtime::stream_path(job.provider, &job.session_id);
        if let Some(rl) = global_claude::last_rate_limit_status(&stream_path) {
            let cred_id = sessions_db::get_credential_id(pool, &job.session_id)
                .await
                .unwrap_or(None);
            tracing::info!(
                module = "captain",
                session_id = %job.session_id,
                credential_id = ?cred_id,
                rl_status = %rl.status,
                rl_type = rl.rate_limit_type.as_deref().unwrap_or("unknown"),
                resets_at = ?rl.resets_at,
                utilization = ?rl.utilization,
                overage = rl.overage_status.as_deref().unwrap_or("none"),
                "session rate-limit status at exit"
            );
        }
    }

    // Phase 2: Terminate all in parallel.
    let reconciled = jobs.len();
    let futs: Vec<_> = jobs
        .iter()
        .map(|job| {
            tracing::info!(
                module = "captain",
                session_id = %job.session_id,
                provider = %job.provider.as_str(),
                status = %job.status.as_str(),
                reason = job.reason,
                "session reconciliation terminating session"
            );
            crate::io::session_terminate::terminate_session(pool, &job.session_id, job.status, None)
        })
        .collect();
    futures::future::join_all(futs).await;

    tracing::info!(
        module = "captain",
        checked = running.len(),
        reconciled,
        "session reconciliation complete"
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use global_db::Db;
    use sessions_db::{upsert_session, SessionRow, SessionUpsert};

    async fn test_pool() -> SqlitePool {
        let db = Db::open_in_memory().await.unwrap();
        db.pool().clone()
    }

    async fn insert_running(pool: &SqlitePool, provider: global_types::TaskProvider, sid: &str) {
        upsert_session(
            pool,
            &SessionUpsert {
                provider,
                session_id: sid,
                created_at: "2026-05-23T00:00:00Z",
                caller: "worker",
                cwd: "/tmp",
                model: "test-model",
                status: SessionStatus::Running,
                cost_usd: None,
                duration_ms: None,
                resumed: false,
                task_id: Some(1),
                scout_item_id: None,
                worker_name: Some("worker-1"),
                resumed_at: None,
                credential_id: None,
                error: None,
                api_error_status: None,
            },
        )
        .await
        .unwrap();
    }

    async fn load_session(pool: &SqlitePool, sid: &str) -> SessionRow {
        sessions_db::session_by_id(pool, sid)
            .await
            .unwrap()
            .unwrap()
    }

    fn isolate_data_dir() -> (std::path::PathBuf, global_infra::EnvVarGuard) {
        let dir = std::env::temp_dir().join(format!(
            "mando-session-reconcile-{}",
            global_infra::uuid::Uuid::v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let guard = global_infra::EnvVarGuard::set("MANDO_DATA_DIR", &dir);
        (dir, guard)
    }

    fn write_stream(provider: global_types::TaskProvider, session_id: &str, content: &str) {
        let path = crate::runtime::agent_runtime::stream_path(provider, session_id);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, content).unwrap();
    }

    #[tokio::test]
    async fn codex_terminal_result_wins_over_live_shared_pid() {
        let _lock = global_infra::PROCESS_ENV_LOCK.lock().await;
        let (_dir, _guard) = isolate_data_dir();
        let pool = test_pool().await;
        let sid = "codex-finished-live-pid";
        insert_running(&pool, global_types::TaskProvider::Codex, sid).await;
        crate::io::pid_registry::register(sid, crate::Pid::new(std::process::id())).unwrap();
        write_stream(
            global_types::TaskProvider::Codex,
            sid,
            concat!(
                r#"{"type":"system","subtype":"init"}"#,
                "\n",
                r#"{"type":"result","subtype":"success","is_error":false,"total_cost_usd":0.01}"#,
                "\n"
            ),
        );
        let mut alerts = Vec::new();

        reconcile_running_sessions(&pool, std::time::Duration::from_secs(60), &mut alerts).await;

        let row = load_session(&pool, sid).await;
        assert_eq!(row.status, "stopped");
        assert_eq!(row.cost_usd, Some(0.01));
        assert!(alerts.is_empty());
    }

    #[tokio::test]
    async fn codex_inactive_missing_stream_fails_even_when_app_server_pid_is_alive() {
        let _lock = global_infra::PROCESS_ENV_LOCK.lock().await;
        let (_dir, _guard) = isolate_data_dir();
        let pool = test_pool().await;
        let sid = "codex-missing-stream-live-pid";
        insert_running(&pool, global_types::TaskProvider::Codex, sid).await;
        crate::io::pid_registry::register(sid, crate::Pid::new(std::process::id())).unwrap();
        let mut alerts = Vec::new();

        reconcile_running_sessions(&pool, std::time::Duration::from_secs(60), &mut alerts).await;

        let row = load_session(&pool, sid).await;
        assert_eq!(row.status, "failed");
        assert!(alerts.is_empty());
    }
}
