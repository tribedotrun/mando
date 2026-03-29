//! Timeline event queries — replaces file-based timeline store.

use anyhow::Result;
use sqlx::SqlitePool;

use mando_types::timeline::{TimelineEvent, TimelineEventType};

#[derive(sqlx::FromRow)]
struct TimelineRow {
    #[allow(dead_code)]
    id: i64,
    #[allow(dead_code)]
    task_id: i64,
    event_type: String,
    timestamp: String,
    actor: String,
    summary: String,
    data: String,
}

impl TimelineRow {
    fn into_event(self) -> TimelineEvent {
        let event_type: TimelineEventType =
            serde_json::from_value(serde_json::Value::String(self.event_type.clone()))
                .unwrap_or(TimelineEventType::StatusChanged);
        let data: serde_json::Value =
            serde_json::from_str(&self.data).unwrap_or(serde_json::Value::Null);
        TimelineEvent {
            event_type,
            timestamp: self.timestamp,
            actor: self.actor,
            summary: self.summary,
            data,
        }
    }
}

/// Append an event to a task's timeline.
pub async fn append(pool: &SqlitePool, task_id: i64, event: &TimelineEvent) -> Result<()> {
    let event_type_str = serde_json::to_value(event.event_type)?
        .as_str()
        .unwrap_or("status_changed")
        .to_string();
    let data_str = serde_json::to_string(&event.data)?;
    sqlx::query(
        "INSERT INTO timeline_events (task_id, event_type, timestamp, actor, summary, data)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )
    .bind(task_id)
    .bind(&event_type_str)
    .bind(&event.timestamp)
    .bind(&event.actor)
    .bind(&event.summary)
    .bind(&data_str)
    .execute(pool)
    .await?;
    Ok(())
}

/// Load all timeline events for a task, ordered chronologically.
pub async fn load(pool: &SqlitePool, task_id: i64) -> Result<Vec<TimelineEvent>> {
    let rows: Vec<TimelineRow> =
        sqlx::query_as("SELECT * FROM timeline_events WHERE task_id = ? ORDER BY timestamp ASC")
            .bind(task_id)
            .fetch_all(pool)
            .await?;
    Ok(rows.into_iter().map(|r| r.into_event()).collect())
}

/// Load the last N timeline events for a task.
pub async fn load_last_n(pool: &SqlitePool, task_id: i64, n: i64) -> Result<Vec<TimelineEvent>> {
    let rows: Vec<TimelineRow> = sqlx::query_as(
        "SELECT * FROM (
            SELECT * FROM timeline_events WHERE task_id = ? ORDER BY timestamp DESC LIMIT ?
         ) ORDER BY timestamp ASC",
    )
    .bind(task_id)
    .bind(n)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|r| r.into_event()).collect())
}

/// Count events for a task.
pub async fn count(pool: &SqlitePool, task_id: i64) -> Result<usize> {
    let c: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM timeline_events WHERE task_id = ?")
        .bind(task_id)
        .fetch_one(pool)
        .await?;
    Ok(c as usize)
}

/// Check if a backfill marker exists for a task.
pub async fn has_backfill_marker(pool: &SqlitePool, task_id: i64) -> Result<bool> {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM timeline_events WHERE task_id = ? AND data LIKE '%\"source\":\"backfill\"%' LIMIT 1)",
    )
    .bind(task_id)
    .fetch_one(pool)
    .await?;
    Ok(exists)
}

/// Delete all timeline events for a task (used during backfill rewrite).
pub async fn delete_all(pool: &SqlitePool, task_id: i64) -> Result<()> {
    sqlx::query("DELETE FROM timeline_events WHERE task_id = ?")
        .bind(task_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Bulk insert events (for backfill).
pub async fn bulk_insert(pool: &SqlitePool, task_id: i64, events: &[TimelineEvent]) -> Result<()> {
    let mut tx = pool.begin().await?;
    for event in events {
        let event_type_str = serde_json::to_value(event.event_type)?
            .as_str()
            .unwrap_or("status_changed")
            .to_string();
        let data_str = serde_json::to_string(&event.data)?;
        sqlx::query(
            "INSERT INTO timeline_events (task_id, event_type, timestamp, actor, summary, data)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )
        .bind(task_id)
        .bind(&event_type_str)
        .bind(&event.timestamp)
        .bind(&event.actor)
        .bind(&event.summary)
        .bind(&data_str)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}
