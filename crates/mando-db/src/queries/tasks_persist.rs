//! Immediate-persist helpers for task fields.
//!
//! Each function writes a narrow set of columns directly to SQLite so the DB
//! reflects in-flight work even if captain crashes before tick-end merge.

use anyhow::Result;
use sqlx::SqlitePool;

use mando_types::task::Task;
use mando_types::timeline::TimelineEvent;

use super::tasks::{bind_task_write_fields, update_set_clause};

/// Immediately persist critical worker fields after spawn.
///
/// Writes critical worker fields so the DB reflects the running worker
/// even if captain crashes before tick-end merge.
pub async fn persist_spawn(pool: &SqlitePool, task: &Task) -> Result<()> {
    sqlx::query(
        "UPDATE tasks SET status=?, worker=?, session_ids=?, worker_started_at=?, \
         worktree=?, plan=? \
         WHERE id=? AND status NOT IN ('merged','completed-no-pr','canceled')",
    )
    .bind(task.status.as_str())
    .bind(&task.worker)
    .bind(task.session_ids.to_json())
    .bind(&task.worker_started_at)
    .bind(&task.worktree)
    .bind(&task.plan)
    .bind(task.id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Immediately persist merge session fields so the DB reflects the running
/// merge session even if captain crashes before tick-end write-back.
pub async fn persist_merge_spawn(pool: &SqlitePool, task: &Task) -> Result<()> {
    let result = sqlx::query(
        "UPDATE tasks SET session_ids=?, last_activity_at=? \
         WHERE id=? AND status = 'captain-merging'",
    )
    .bind(task.session_ids.to_json())
    .bind(&task.last_activity_at)
    .bind(task.id)
    .execute(pool)
    .await?;
    anyhow::ensure!(
        result.rows_affected() > 0,
        "persist_merge_spawn: 0 rows affected for task {} — status changed concurrently",
        task.id,
    );
    Ok(())
}

/// Persist clarifier start fields immediately so the UI reflects the running
/// clarifier while it's still in progress.
pub async fn persist_clarify_start(pool: &SqlitePool, task: &Task) -> Result<()> {
    sqlx::query(
        "UPDATE tasks SET status=?, session_ids=? \
         WHERE id=? AND status NOT IN ('merged','completed-no-pr','canceled')",
    )
    .bind(task.status.as_str())
    .bind(task.session_ids.to_json())
    .bind(task.id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Immediately persist clarifier result fields so the DB reflects the
/// enriched context even if captain crashes before tick-end merge.
pub async fn persist_clarify_result(pool: &SqlitePool, task: &Task) -> Result<()> {
    let trigger_str = task.captain_review_trigger.map(|t| t.as_str().to_string());
    sqlx::query(
        "UPDATE tasks SET status=?, context=?, title=?, session_ids=?, project=?, \
         no_pr=?, resource=?, clarifier_fail_count=?, last_activity_at=?, \
         captain_review_trigger=?, review_fail_count=? \
         WHERE id=? AND status IN ('clarifying','needs-clarification')",
    )
    .bind(task.status.as_str())
    .bind(&task.context)
    .bind(&task.title)
    .bind(task.session_ids.to_json())
    .bind(&task.project)
    .bind(task.no_pr as i64)
    .bind(&task.resource)
    .bind(task.clarifier_fail_count)
    .bind(&task.last_activity_at)
    .bind(&trigger_str)
    .bind(task.review_fail_count)
    .bind(task.id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Atomically persist a status transition + timeline event in one SQLite transaction.
///
/// 1. Updates the full task row with a `WHERE id=? AND status=?` guard so the
///    write is a no-op if the task already transitioned (idempotent).
/// 2. Inserts the corresponding timeline event (with a dedupe key) in the same tx.
/// 3. Commits both atomically.
///
/// Returns `Ok(true)` when the transition was applied, `Ok(false)` when the guard
/// prevented it (already transitioned), and `Err` on DB failure (nothing committed).
///
/// Callers should update in-memory state and send notifications only on `Ok(true)`.
pub async fn persist_status_transition(
    pool: &SqlitePool,
    task: &Task,
    expected_status: &str,
    event: &TimelineEvent,
) -> Result<bool> {
    let event_type_str = super::timeline::event_type_to_string(event.event_type)?;
    let data_str = serde_json::to_string(&event.data)?;
    let dedupe_key = format!(
        "{}-{}-from:{}-w{}-r{}-i{}-rf{}",
        task.id,
        event_type_str,
        expected_status,
        task.worker_seq,
        task.reopen_seq,
        task.intervention_count,
        task.review_fail_count,
    );

    let mut tx = pool.begin().await?;

    // Update task with guard condition. Uses the same field set as update_task_exec
    // but with `WHERE status = expected_status` as an idempotency guard.
    let set_clause = update_set_clause();
    let result = bind_task_write_fields(
        sqlx::query(&format!(
            "UPDATE tasks SET {set_clause} WHERE id=? AND status=?"
        )),
        task,
    )
    .bind(task.id)
    .bind(expected_status)
    .execute(&mut *tx)
    .await?;

    if result.rows_affected() == 0 {
        tx.rollback().await?;
        tracing::info!(
            module = "task-store",
            task_id = task.id,
            expected_status,
            new_status = task.status.as_str(),
            "persist_status_transition: 0 rows — already transitioned"
        );
        return Ok(false);
    }

    // Insert timeline event with dedupe key in the same transaction.
    sqlx::query(&format!(
        "INSERT INTO timeline_events ({}) VALUES (?, ?, ?, ?, ?, ?, ?)",
        super::timeline::INSERT_COLS
    ))
    .bind(task.id)
    .bind(&event_type_str)
    .bind(&event.timestamp)
    .bind(&event.actor)
    .bind(&event.summary)
    .bind(&data_str)
    .bind(&dedupe_key)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    tracing::info!(
        module = "task-store",
        task_id = task.id,
        new_status = task.status.as_str(),
        event_type = %event_type_str,
        "persist_status_transition: committed"
    );

    Ok(true)
}
