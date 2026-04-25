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
//! - Tasks stranded in `Clarifying` from a prior daemon run are reverted
//!   to `NeedsClarification` so the human can re-answer the outstanding
//!   question. This replaces what the deleted `dispatch_reclarify`
//!   tick-path safety net used to handle during normal operation.

use anyhow::{Context, Result};
use api_types::TimelineEventPayload;
use global_types::SessionStatus;
use sqlx::SqlitePool;

use crate::io::session_terminate::terminate_session;
use crate::service::lifecycle;
use crate::{ItemStatus, TimelineEvent};

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

/// Unstick any task stranded in `Clarifying` from a prior daemon run.
///
/// With the `dispatch_reclarify` safety net removed and
/// `persist_resume_clarifier` no longer nulling the clarifier session id,
/// the only way a task ends a daemon lifetime in `Clarifying` is a daemon
/// crash during the HTTP inline reclarifier call. The stream file for its
/// clarifier session either never completed or carries a result that has
/// already been applied to the task. Either way, the task is not reachable
/// by any live writer after restart:
///
/// - `tick_clarify_poll` skips sessions whose `result_applied_at` is set
///   (idempotency guard), so it will not re-apply the prior round's
///   already-consumed stream on top of the unanswered human turn.
/// - Nothing else polls or dispatches follow-up clarifiers.
///
/// This function walks those tasks once and reverts them to
/// `NeedsClarification`. The outstanding question from the prior
/// `ClarifyQuestion` timeline event is still visible in the UI, so the
/// human can resend their answer and the HTTP inline path takes over
/// cleanly. Logs and a `ClarifierFailed` timeline event record the
/// recovery for postmortem.
///
/// Non-fatal: a single task that fails to revert is logged and the loop
/// continues. Daemon boot is not blocked on a corrupt row.
#[tracing::instrument(skip_all)]
pub async fn reconcile_stranded_clarifying_tasks(pool: &SqlitePool) {
    let tasks = match crate::io::queries::tasks::load_all(pool).await {
        Ok(t) => t,
        Err(e) => {
            tracing::error!(
                module = "startup",
                error = %e,
                "failed to load tasks — skipping stranded-clarifier reconcile"
            );
            return;
        }
    };

    let mut recovered = 0u32;
    for task in tasks {
        if task.status != ItemStatus::Clarifying {
            continue;
        }

        // Classify why this task is stranded so the recovery log line is
        // useful. Three shapes are possible in practice:
        //   (a) no clarifier session id — legacy DB from before this PR,
        //       or a previous writer nulled it and crashed;
        //   (b) session has `result_applied_at` set — daemon died after a
        //       successful prior round's apply and before the next CC call
        //       returned;
        //   (c) session exists but has no applied marker — the prior
        //       `reconcile_startup_sessions` has already set the row to
        //       Stopped or Failed; the next `tick_clarify_poll` will read
        //       the stream file and apply/revert accordingly, so skip here.
        //       A transient DB error loading the cc_sessions row leaves the
        //       task stuck until the next restart (unrecoverable from a
        //       persistently-broken schema; recoverable from a transient
        //       blip because tick_clarify_poll retries on every tick).
        let reason = match task.session_ids.clarifier.as_deref() {
            None => "no clarifier session id",
            Some(sid) => match sessions_db::session_by_id(pool, sid).await {
                Ok(Some(row)) if row.result_applied_at.is_some() => {
                    "prior round's result already applied"
                }
                Ok(Some(_)) => {
                    // Session result not yet applied — tick_clarify_poll
                    // will pick up the stream file and process it.
                    continue;
                }
                Ok(None) => "session id points to missing cc_sessions row",
                Err(e) => {
                    tracing::warn!(
                        module = "startup",
                        task_id = task.id,
                        %sid,
                        error = %e,
                        "failed to load cc_sessions row for stranded-clarifier reconcile \
                         — task stays in Clarifying until next tick's retry or next restart"
                    );
                    continue;
                }
            },
        };

        if let Err(e) = revert_stranded_task_to_needs_clarification(pool, &task, reason).await {
            tracing::warn!(
                module = "startup",
                task_id = task.id,
                error = %e,
                reason,
                "failed to revert stranded Clarifying task — leaving in place for manual recovery"
            );
        } else {
            recovered += 1;
        }
    }

    if recovered > 0 {
        tracing::info!(
            module = "startup",
            recovered,
            "recovered stranded Clarifying tasks (reverted to NeedsClarification)"
        );
    }
}

async fn revert_stranded_task_to_needs_clarification(
    pool: &SqlitePool,
    task: &crate::Task,
    reason: &str,
) -> Result<()> {
    let mut next = task.clone();
    lifecycle::apply_clarifier_failure(&mut next)?;
    let message = format!("Stranded clarifier recovered after daemon restart ({reason})");
    let event = TimelineEvent {
        timestamp: global_types::now_rfc3339(),
        actor: "startup".to_string(),
        summary: message.clone(),
        data: TimelineEventPayload::ClarifierFailed {
            session_id: task.session_ids.clarifier.clone().unwrap_or_default(),
            api_error_status: 0,
            message,
        },
    };
    let applied = crate::io::queries::tasks::persist_status_transition_with_command(
        pool,
        &next,
        ItemStatus::Clarifying.as_str(),
        "needs_clarification",
        &event,
    )
    .await?;
    anyhow::ensure!(
        applied,
        "persist_status_transition_with_command rejected revert for task {}",
        task.id
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

    // ── reconcile_stranded_clarifying_tasks tests ────────────────────────────

    use crate::Task;
    use sessions_db::mark_session_result_applied;

    async fn pool_with_project() -> SqlitePool {
        let db = Db::open_in_memory().await.unwrap();
        settings::projects::upsert(db.pool(), "test", "", None)
            .await
            .unwrap();
        db.pool().clone()
    }

    fn clarifying_task(title: &str) -> Task {
        let mut task = Task::new(title);
        task.project_id = 1;
        task.project = "test".into();
        task.status = ItemStatus::Clarifying;
        task.last_activity_at = Some(global_types::now_rfc3339());
        task
    }

    async fn insert_applied_session(pool: &SqlitePool, sid: &str, task_id: i64) {
        upsert_session(
            pool,
            &SessionUpsert {
                session_id: sid,
                created_at: "2026-04-24T00:00:00Z",
                caller: "clarifier",
                cwd: "/tmp",
                model: "opus",
                status: SessionStatus::Stopped,
                cost_usd: Some(0.1),
                duration_ms: Some(1000),
                resumed: false,
                task_id: Some(task_id),
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
        mark_session_result_applied(pool, sid).await.unwrap();
    }

    async fn insert_live_session(pool: &SqlitePool, sid: &str, task_id: i64) {
        upsert_session(
            pool,
            &SessionUpsert {
                session_id: sid,
                created_at: "2026-04-24T00:00:00Z",
                caller: "clarifier",
                cwd: "/tmp",
                model: "opus",
                status: SessionStatus::Running,
                cost_usd: None,
                duration_ms: None,
                resumed: false,
                task_id: Some(task_id),
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
        // NOT calling mark_session_result_applied — this is a live mid-run session.
    }

    #[tokio::test]
    async fn stranded_no_session_id_reverts_to_needs_clarification() {
        // Stream-missing case: task is Clarifying but has no clarifier
        // session id at all (legacy-DB shape or pre-PR writer crashed
        // after nulling but before the inline reclarifier set a new one).
        let pool = pool_with_project().await;
        let mut task = clarifying_task("no-session");
        task.session_ids.clarifier = None;
        let id = crate::io::queries::tasks::insert_task(&pool, &task)
            .await
            .unwrap();

        reconcile_stranded_clarifying_tasks(&pool).await;

        let after = crate::io::queries::tasks::find_by_id(&pool, id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(after.status, ItemStatus::NeedsClarification);
        assert!(after.session_ids.clarifier.is_none());
    }

    #[tokio::test]
    async fn stranded_applied_session_reverts_to_needs_clarification() {
        // Stream-complete case: the clarifier session's result was already
        // applied in a prior round (result_applied_at is set). The daemon
        // then died during the follow-up inline call, leaving the task
        // stuck in Clarifying. Revert so the user resends their answer.
        let pool = pool_with_project().await;
        let mut task = clarifying_task("applied-session");
        task.session_ids.clarifier = Some("clarifier-applied".into());
        let id = crate::io::queries::tasks::insert_task(&pool, &task)
            .await
            .unwrap();
        insert_applied_session(&pool, "clarifier-applied", id).await;

        reconcile_stranded_clarifying_tasks(&pool).await;

        let after = crate::io::queries::tasks::find_by_id(&pool, id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(after.status, ItemStatus::NeedsClarification);
        // `apply_clarifier_failure` also clears the session id so the next
        // user answer starts a fresh CC run rather than trying to resume
        // the already-consumed session.
        assert!(after.session_ids.clarifier.is_none());
    }

    #[tokio::test]
    async fn stranded_live_session_left_for_tick() {
        // Stream-live case: the clarifier session is unfinished and has no
        // applied marker. The first captain tick's `tick_clarify_poll`
        // will pick it up via the stream file. Startup must NOT revert.
        //
        // In production, `reconcile_startup_sessions` runs before
        // `reconcile_stranded_clarifying_tasks`, so by the time this
        // function runs the session is already Stopped or Failed (not
        // Running). This test inserts a Running session via
        // `insert_live_session` because the specific status doesn't matter
        // for the branch under test — what matters is
        // `result_applied_at.is_some()` (false here, forcing the continue
        // branch). The test proves the branch condition; the full
        // startup ordering is covered by the session-level reconcile
        // tests above.
        let pool = pool_with_project().await;
        let mut task = clarifying_task("live-session");
        task.session_ids.clarifier = Some("clarifier-live".into());
        let id = crate::io::queries::tasks::insert_task(&pool, &task)
            .await
            .unwrap();
        insert_live_session(&pool, "clarifier-live", id).await;

        reconcile_stranded_clarifying_tasks(&pool).await;

        let after = crate::io::queries::tasks::find_by_id(&pool, id)
            .await
            .unwrap()
            .unwrap();
        // Status and session id untouched — tick_clarify_poll owns this case.
        assert_eq!(after.status, ItemStatus::Clarifying);
        assert_eq!(
            after.session_ids.clarifier.as_deref(),
            Some("clarifier-live")
        );
    }
}
