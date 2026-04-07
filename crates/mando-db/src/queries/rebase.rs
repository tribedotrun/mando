//! Rebase state queries — manages the task_rebase_state sub-table.

use anyhow::Result;
use sqlx::SqlitePool;

use mando_types::rebase_state::RebaseState;

const SELECT_COLS: &str = "task_id, worker, status, retries, head_sha";

#[derive(sqlx::FromRow)]
struct RebaseRow {
    task_id: i64,
    worker: Option<String>,
    status: String,
    retries: i64,
    head_sha: Option<String>,
}

impl RebaseRow {
    fn into_state(self) -> Result<RebaseState> {
        let status = self.status.parse().map_err(|e| {
            tracing::error!(
                module = "rebase-db",
                task_id = self.task_id,
                status = %self.status,
                error = %e,
                "failed to parse rebase status",
            );
            anyhow::anyhow!(
                "rebase row {} has invalid status {:?}: {e}",
                self.task_id,
                self.status,
            )
        })?;
        Ok(RebaseState {
            task_id: self.task_id,
            worker: self.worker,
            status,
            retries: self.retries,
            head_sha: self.head_sha,
        })
    }
}

/// Get rebase state for a task (None if no rebase has been attempted).
pub async fn get(pool: &SqlitePool, task_id: i64) -> Result<Option<RebaseState>> {
    let row: Option<RebaseRow> = sqlx::query_as(&format!(
        "SELECT {SELECT_COLS} FROM task_rebase_state WHERE task_id = ?"
    ))
    .bind(task_id)
    .fetch_optional(pool)
    .await?;
    row.map(|r| r.into_state()).transpose()
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
    let rows: Vec<RebaseRow> =
        sqlx::query_as(&format!("SELECT {SELECT_COLS} FROM task_rebase_state"))
            .fetch_all(pool)
            .await?;
    rows.into_iter().map(|r| r.into_state()).collect()
}
