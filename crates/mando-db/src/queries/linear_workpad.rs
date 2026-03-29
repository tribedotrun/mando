//! Linear workpad comment mapping queries — replaces file-based store.

use std::collections::HashMap;

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

/// Load all mappings.
pub async fn load_all(pool: &SqlitePool) -> Result<HashMap<String, String>> {
    let rows: Vec<(String, String)> =
        sqlx::query_as("SELECT linear_id, comment_id FROM linear_workpad")
            .fetch_all(pool)
            .await?;
    Ok(rows.into_iter().collect())
}

/// Delete a mapping.
pub async fn delete(pool: &SqlitePool, linear_id: &str) -> Result<bool> {
    let result = sqlx::query("DELETE FROM linear_workpad WHERE linear_id = ?")
        .bind(linear_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}
