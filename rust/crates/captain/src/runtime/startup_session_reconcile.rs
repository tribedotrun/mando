//! Startup reconciliation for in-flight CC sessions.
//!
//! On daemon restart, any `cc_sessions` row still marked `running` belongs
//! to a subprocess that either never existed after restart or was an
//! orphan killed by `pid_registry::cleanup_on_startup`. This module walks
//! every such row, decides whether the session produced a usable stream
//! result, and calls `terminate_session` with the appropriate terminal
//! status. Task-side salvage (promoting a clarifier/review/merge result
//! into task state) happens automatically on the first captain tick,
//! which already reads stream files via `tick_clarify_poll`,
//! `captain_review_check`, and `captain_merge_poll`.
//!
//! Expected behaviour after this runs:
//! - No `cc_sessions` row remains in `running` state.
//! - Sessions whose stream ends with a clean `result` event are marked
//!   `Stopped` with cost/duration backfilled.
//! - Sessions whose stream has an error or no result at all are marked
//!   `Failed`.
//! - First captain tick (<=60s later) salvages stopped-with-result
//!   sessions into their parent tasks. Failed sessions trigger a fresh
//!   clean-slate respawn on the same tick.

use anyhow::{Context, Result};
use global_types::SessionStatus;
use sqlx::SqlitePool;

use crate::io::session_terminate::terminate_session;

/// Reconcile every session currently marked `running`.
///
/// Returns an error when the enumerating query itself fails so the
/// caller (captain's `reconcile_on_startup`) can propagate via `?`
/// and let the daemon's `MANDO_UNSAFE_START` gate decide whether to
/// abort boot. Per-row `terminate_session` calls do their own
/// structured logging on failure and do not stop the loop; a single
/// corrupt stream file should not block recovery of the other rows.
///
/// Assumes `pid_registry::cleanup_on_startup` has already run so PID
/// kill inside `terminate_session` is effectively a no-op.
#[tracing::instrument(skip_all)]
pub async fn reconcile_startup_sessions(pool: &SqlitePool) -> Result<()> {
    let running = sessions_db::list_running_sessions(pool)
        .await
        .context("startup session reconciliation: list_running_sessions failed")?;
    if running.is_empty() {
        return Ok(());
    }

    let mut salvaged = 0u32;
    let mut failed = 0u32;
    for row in &running {
        let sid = row.session_id.as_str();
        let stream_path = global_infra::paths::stream_path_for_session(sid);
        let status = match global_claude::get_stream_result(&stream_path) {
            Some(result) if global_claude::is_clean_result(&result) => {
                salvaged += 1;
                SessionStatus::Stopped
            }
            _ => {
                failed += 1;
                SessionStatus::Failed
            }
        };
        terminate_session(pool, sid, status, None).await;
    }

    tracing::info!(
        module = "startup",
        total = running.len(),
        salvaged,
        failed,
        "reconciled in-flight sessions from prior daemon"
    );
    Ok(())
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

    async fn insert_running(pool: &SqlitePool, sid: &str, caller: &str, task_id: Option<i64>) {
        upsert_session(
            pool,
            &SessionUpsert {
                session_id: sid,
                created_at: "2026-04-14T00:00:00Z",
                caller,
                cwd: "/tmp",
                model: "opus",
                status: SessionStatus::Running,
                cost_usd: None,
                duration_ms: None,
                resumed: false,
                task_id,
                scout_item_id: None,
                worker_name: None,
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
            "mando-startup-reconcile-{}",
            global_infra::uuid::Uuid::v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let guard = global_infra::EnvVarGuard::set("MANDO_DATA_DIR", &dir);
        (dir, guard)
    }

    fn write_stream(session_id: &str, content: &str) {
        let path = global_infra::paths::stream_path_for_session(session_id);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, content).unwrap();
    }

    #[tokio::test]
    async fn clean_result_marks_stopped() {
        let _lock = global_infra::PROCESS_ENV_LOCK.lock().await;
        let (_dir, _guard) = isolate_data_dir();
        let pool = test_pool().await;
        insert_running(&pool, "s-clean", "clarifier", Some(1)).await;
        write_stream(
            "s-clean",
            concat!(
                r#"{"type":"system","subtype":"init"}"#,
                "\n",
                r#"{"type":"result","subtype":"success","total_cost_usd":0.5,"duration_ms":12000}"#,
                "\n",
            ),
        );

        reconcile_startup_sessions(&pool).await.unwrap();

        let row = load_session(&pool, "s-clean").await;
        assert_eq!(row.status, "stopped");
        assert_eq!(row.cost_usd, Some(0.5));
        assert_eq!(row.duration_ms, Some(12000));
    }

    #[tokio::test]
    async fn error_result_marks_failed() {
        let _lock = global_infra::PROCESS_ENV_LOCK.lock().await;
        let (_dir, _guard) = isolate_data_dir();
        let pool = test_pool().await;
        insert_running(&pool, "s-err", "clarifier", Some(2)).await;
        write_stream(
            "s-err",
            concat!(
                r#"{"type":"system","subtype":"init"}"#,
                "\n",
                r#"{"type":"result","subtype":"error_max_turns","is_error":true}"#,
                "\n",
            ),
        );

        reconcile_startup_sessions(&pool).await.unwrap();

        let row = load_session(&pool, "s-err").await;
        assert_eq!(row.status, "failed");
    }

    #[tokio::test]
    async fn missing_result_marks_failed() {
        let _lock = global_infra::PROCESS_ENV_LOCK.lock().await;
        let (_dir, _guard) = isolate_data_dir();
        let pool = test_pool().await;
        insert_running(&pool, "s-none", "worker", Some(3)).await;
        // Stream file exists but has no result event.
        write_stream(
            "s-none",
            concat!(
                r#"{"type":"system","subtype":"init"}"#,
                "\n",
                r#"{"type":"assistant","message":{"content":[]}}"#,
                "\n",
            ),
        );

        reconcile_startup_sessions(&pool).await.unwrap();

        let row = load_session(&pool, "s-none").await;
        assert_eq!(row.status, "failed");
    }

    #[tokio::test]
    async fn empty_running_noop() {
        let _lock = global_infra::PROCESS_ENV_LOCK.lock().await;
        let (_dir, _guard) = isolate_data_dir();
        let pool = test_pool().await;
        // No running sessions; must not panic or error.
        reconcile_startup_sessions(&pool).await.unwrap();
        let running = sessions_db::list_running_sessions(&pool).await.unwrap();
        assert!(running.is_empty());
    }

    #[tokio::test]
    async fn db_error_propagates_to_caller() {
        let _lock = global_infra::PROCESS_ENV_LOCK.lock().await;
        let (_dir, _guard) = isolate_data_dir();
        // Open a pool, then close it before calling reconcile so the DB
        // query fails with a predictable error. This proves the
        // MANDO_UNSAFE_START gate at the caller can observe a failure
        // instead of silently booting against stale state.
        let pool = test_pool().await;
        pool.close().await;
        let err = reconcile_startup_sessions(&pool).await.unwrap_err();
        assert!(
            err.to_string().contains("list_running_sessions failed"),
            "unexpected error: {err}"
        );
    }
}
