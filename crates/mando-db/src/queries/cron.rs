//! Cron job queries — replaces file-based cron store.

use anyhow::Result;
use sqlx::SqlitePool;

use mando_types::cron::CronJob;

#[derive(sqlx::FromRow)]
struct CronRow {
    id: String,
    name: String,
    enabled: i64,
    schedule_json: String,
    payload_json: String,
    state_json: String,
    created_at_ms: i64,
    updated_at_ms: i64,
    delete_after_run: i64,
    job_type: String,
    cwd: Option<String>,
    timeout_s: i64,
}

impl CronRow {
    fn into_job(self) -> CronJob {
        CronJob {
            id: self.id,
            name: self.name,
            enabled: self.enabled != 0,
            schedule: serde_json::from_str(&self.schedule_json).unwrap_or_default(),
            payload: serde_json::from_str(&self.payload_json).unwrap_or_default(),
            state: serde_json::from_str(&self.state_json).unwrap_or_default(),
            created_at_ms: self.created_at_ms,
            updated_at_ms: self.updated_at_ms,
            delete_after_run: self.delete_after_run != 0,
            job_type: self.job_type,
            cwd: self.cwd,
            timeout_s: self.timeout_s,
        }
    }
}

/// Load all cron jobs.
pub async fn load_all(pool: &SqlitePool) -> Result<Vec<CronJob>> {
    let rows: Vec<CronRow> = sqlx::query_as("SELECT * FROM cron_jobs")
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(|r| r.into_job()).collect())
}

/// Get a single cron job by ID.
pub async fn get(pool: &SqlitePool, id: &str) -> Result<Option<CronJob>> {
    let row: Option<CronRow> = sqlx::query_as("SELECT * FROM cron_jobs WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|r| r.into_job()))
}

/// Upsert a cron job.
pub async fn upsert(pool: &SqlitePool, job: &CronJob) -> Result<()> {
    let schedule_json = serde_json::to_string(&job.schedule)?;
    let payload_json = serde_json::to_string(&job.payload)?;
    let state_json = serde_json::to_string(&job.state)?;
    sqlx::query(
        "INSERT INTO cron_jobs (id, name, enabled, schedule_json, payload_json, state_json,
            created_at_ms, updated_at_ms, delete_after_run, job_type, cwd, timeout_s)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)
         ON CONFLICT(id) DO UPDATE SET
            name=excluded.name, enabled=excluded.enabled, schedule_json=excluded.schedule_json,
            payload_json=excluded.payload_json, state_json=excluded.state_json,
            updated_at_ms=excluded.updated_at_ms, delete_after_run=excluded.delete_after_run,
            job_type=excluded.job_type, cwd=excluded.cwd, timeout_s=excluded.timeout_s",
    )
    .bind(&job.id)
    .bind(&job.name)
    .bind(job.enabled as i64)
    .bind(&schedule_json)
    .bind(&payload_json)
    .bind(&state_json)
    .bind(job.created_at_ms)
    .bind(job.updated_at_ms)
    .bind(job.delete_after_run as i64)
    .bind(&job.job_type)
    .bind(&job.cwd)
    .bind(job.timeout_s)
    .execute(pool)
    .await?;
    Ok(())
}

/// Delete a cron job by ID.
pub async fn delete(pool: &SqlitePool, id: &str) -> Result<bool> {
    let result = sqlx::query("DELETE FROM cron_jobs WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

/// Replace all cron jobs atomically.
pub async fn replace_all(pool: &SqlitePool, jobs: &[CronJob]) -> Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM cron_jobs")
        .execute(&mut *tx)
        .await?;
    for job in jobs {
        let schedule_json = serde_json::to_string(&job.schedule)?;
        let payload_json = serde_json::to_string(&job.payload)?;
        let state_json = serde_json::to_string(&job.state)?;
        sqlx::query(
            "INSERT INTO cron_jobs (id, name, enabled, schedule_json, payload_json, state_json,
                created_at_ms, updated_at_ms, delete_after_run, job_type, cwd, timeout_s)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
        )
        .bind(&job.id)
        .bind(&job.name)
        .bind(job.enabled as i64)
        .bind(&schedule_json)
        .bind(&payload_json)
        .bind(&state_json)
        .bind(job.created_at_ms)
        .bind(job.updated_at_ms)
        .bind(job.delete_after_run as i64)
        .bind(&job.job_type)
        .bind(&job.cwd)
        .bind(job.timeout_s)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}
