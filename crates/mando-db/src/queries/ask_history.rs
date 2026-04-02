//! Ask history queries — replaces file-based ask history store.

use anyhow::Result;
use sqlx::SqlitePool;

use mando_types::AskHistoryEntry;

#[derive(sqlx::FromRow)]
struct AskHistoryRow {
    role: String,
    content: String,
    timestamp: String,
}

impl AskHistoryRow {
    fn into_entry(self) -> AskHistoryEntry {
        AskHistoryEntry {
            role: self.role,
            content: self.content,
            timestamp: self.timestamp,
        }
    }
}

/// Append an entry to a task's ask history.
pub async fn append(pool: &SqlitePool, task_id: i64, entry: &AskHistoryEntry) -> Result<()> {
    sqlx::query(
        "INSERT INTO ask_history (task_id, role, content, timestamp)
         VALUES (?1, ?2, ?3, ?4)",
    )
    .bind(task_id)
    .bind(&entry.role)
    .bind(&entry.content)
    .bind(&entry.timestamp)
    .execute(pool)
    .await?;
    Ok(())
}

/// Load all ask history for a task, ordered chronologically.
pub async fn load(pool: &SqlitePool, task_id: i64) -> Result<Vec<AskHistoryEntry>> {
    let rows: Vec<AskHistoryRow> = sqlx::query_as(
        "SELECT role, content, timestamp FROM ask_history WHERE task_id = ? ORDER BY timestamp ASC",
    )
    .bind(task_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|r| r.into_entry()).collect())
}
