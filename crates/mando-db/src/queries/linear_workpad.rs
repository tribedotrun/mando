//! Linear workpad comment mapping queries — replaces file-based store.

use anyhow::Result;
use sqlx::SqlitePool;

/// Get the workpad comment ID for a Linear issue.
pub async fn get(pool: &SqlitePool, linear_id: &str) -> Result<Option<String>> {
    let row: Option<(String,)> =
        sqlx::query_as("SELECT comment_id FROM linear_workpad WHERE linear_id = ?")
            .bind(linear_id)
            .fetch_optional(pool)
            .await?;
    Ok(row.map(|(c,)| c))
}

/// Upsert a workpad comment mapping.
pub async fn upsert(pool: &SqlitePool, linear_id: &str, comment_id: &str) -> Result<()> {
    sqlx::query(
        "INSERT INTO linear_workpad (linear_id, comment_id) VALUES (?1, ?2)
         ON CONFLICT(linear_id) DO UPDATE SET comment_id = excluded.comment_id",
    )
    .bind(linear_id)
    .bind(comment_id)
    .execute(pool)
    .await?;
    Ok(())
}
