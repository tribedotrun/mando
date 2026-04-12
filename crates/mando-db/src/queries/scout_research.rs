//! Scout research run queries.

use anyhow::Result;
use mando_types::{ResearchRunStatus, ScoutResearchRun};
use sqlx::SqlitePool;

/// Insert a new research run (status=running), return its ID.
pub async fn insert_run(pool: &SqlitePool, prompt: &str) -> Result<i64> {
    let now = mando_types::now_rfc3339();
    let result = sqlx::query(
        "INSERT INTO scout_research_runs (research_prompt, status, created_at) VALUES (?, 'running', ?)",
    )
    .bind(prompt)
    .bind(&now)
    .execute(pool)
    .await?;
    Ok(result.last_insert_rowid())
}

/// Mark a research run as completed.
pub async fn complete_run(
    pool: &SqlitePool,
    id: i64,
    session_id: &str,
    added_count: i64,
) -> Result<()> {
    let now = mando_types::now_rfc3339();
    sqlx::query(
        "UPDATE scout_research_runs SET status = 'done', session_id = ?, added_count = ?, completed_at = ? WHERE id = ?",
    )
    .bind(session_id)
    .bind(added_count)
    .bind(&now)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Mark a research run as failed.
pub async fn fail_run(pool: &SqlitePool, id: i64, error: &str) -> Result<()> {
    let now = mando_types::now_rfc3339();
    sqlx::query(
        "UPDATE scout_research_runs SET status = 'failed', error = ?, completed_at = ? WHERE id = ?",
    )
    .bind(error)
    .bind(&now)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Get a research run by ID.
pub async fn get_run(pool: &SqlitePool, id: i64) -> Result<Option<ScoutResearchRun>> {
    let row: Option<RunRow> = sqlx::query_as(
        "SELECT id, research_prompt, status, error, session_id, added_count, created_at, completed_at FROM scout_research_runs WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| r.into_run()))
}

/// Mark all runs stuck at `running` as failed (called on startup to
/// recover from daemon crashes that left orphan rows behind).
pub async fn reset_stale_running(pool: &SqlitePool) -> Result<u64> {
    let now = mando_types::now_rfc3339();
    let result = sqlx::query(
        "UPDATE scout_research_runs SET status = 'failed', error = 'interrupted by daemon restart', completed_at = ? WHERE status = 'running'",
    )
    .bind(&now)
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// List recent research runs.
pub async fn list_runs(pool: &SqlitePool, limit: i64) -> Result<Vec<ScoutResearchRun>> {
    let rows: Vec<RunRow> = sqlx::query_as(
        "SELECT id, research_prompt, status, error, session_id, added_count, created_at, completed_at FROM scout_research_runs ORDER BY id DESC LIMIT ?",
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|r| r.into_run()).collect())
}

#[derive(sqlx::FromRow)]
struct RunRow {
    id: i64,
    research_prompt: String,
    status: String,
    error: Option<String>,
    session_id: Option<String>,
    added_count: i64,
    created_at: String,
    completed_at: Option<String>,
}

impl RunRow {
    fn into_run(self) -> ScoutResearchRun {
        let status = self
            .status
            .parse::<ResearchRunStatus>()
            .unwrap_or(ResearchRunStatus::Running);
        ScoutResearchRun {
            id: self.id,
            research_prompt: self.research_prompt,
            status,
            error: self.error,
            session_id: self.session_id,
            added_count: self.added_count,
            created_at: self.created_at,
            completed_at: self.completed_at,
        }
    }
}
