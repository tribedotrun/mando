//! Rebase state queries — manages the task_rebase_state sub-table.

use anyhow::Result;
use sqlx::SqlitePool;

use mando_types::rebase_state::{RebaseState, RebaseStatus};

#[derive(sqlx::FromRow)]
struct RebaseRow {
    task_id: i64,
    worker: Option<String>,
    status: String,
    retries: i64,
    head_sha: Option<String>,
}

impl RebaseRow {
    fn into_state(self) -> RebaseState {
        RebaseState {
            task_id: self.task_id,
            worker: self.worker,
            status: self.status.parse().unwrap_or_default(),
            retries: self.retries,
            head_sha: self.head_sha,
        }
    }
}

/// Get rebase state for a task (None if no rebase has been attempted).
pub async fn get(pool: &SqlitePool, task_id: i64) -> Result<Option<RebaseState>> {
    let row: Option<RebaseRow> =
        sqlx::query_as("SELECT * FROM task_rebase_state WHERE task_id = ?")
            .bind(task_id)
            .fetch_optional(pool)
            .await?;
    Ok(row.map(|r| r.into_state()))
}

/// Upsert rebase state for a task.
pub async fn upsert(pool: &SqlitePool, state: &RebaseState) -> Result<()> {
    sqlx::query(
        "INSERT INTO task_rebase_state (task_id, worker, status, retries, head_sha)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT(task_id) DO UPDATE SET
            worker = excluded.worker,
            status = excluded.status,
            retries = excluded.retries,
            head_sha = excluded.head_sha",
    )
    .bind(state.task_id)
    .bind(&state.worker)
    .bind(state.status.as_str())
    .bind(state.retries)
    .bind(&state.head_sha)
    .execute(pool)
    .await?;
    Ok(())
}

/// Delete rebase state for a task (on rework/clear).
pub async fn delete(pool: &SqlitePool, task_id: i64) -> Result<()> {
    sqlx::query("DELETE FROM task_rebase_state WHERE task_id = ?")
        .bind(task_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Get all rebase state rows (for hydration on load).
pub async fn all(pool: &SqlitePool) -> Result<Vec<RebaseState>> {
    let rows: Vec<RebaseRow> = sqlx::query_as("SELECT * FROM task_rebase_state")
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(|r| r.into_state()).collect())
}

/// Get all tasks with active rebase state.
pub async fn all_active(pool: &SqlitePool) -> Result<Vec<RebaseState>> {
    let rows: Vec<RebaseRow> = sqlx::query_as(
        "SELECT * FROM task_rebase_state WHERE status NOT IN ('succeeded', 'failed')",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|r| r.into_state()).collect())
}

/// Check if a task needs a rebase check (no active rebase worker).
pub async fn needs_rebase_check(pool: &SqlitePool, task_id: i64) -> Result<bool> {
    let state = get(pool, task_id).await?;
    match state {
        None => Ok(true),
        Some(s) => Ok(s.status == RebaseStatus::Failed || s.worker.is_none()),
    }
}

/// Check if rebase failed.
pub async fn is_failed(pool: &SqlitePool, task_id: i64) -> Result<bool> {
    let state = get(pool, task_id).await?;
    Ok(state
        .map(|s| s.status == RebaseStatus::Failed)
        .unwrap_or(false))
}
