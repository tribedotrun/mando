//! Timeline event queries.

use anyhow::Result;
use sqlx::SqlitePool;

use mando_types::timeline::{TimelineEvent, TimelineEventType};

pub(crate) const INSERT_COLS: &str =
    "task_id, event_type, timestamp, actor, summary, data, dedupe_key";

#[derive(sqlx::FromRow)]
struct TimelineRow {
    event_type: String,
    timestamp: String,
    actor: String,
    summary: String,
    data: String,
}

impl TimelineRow {
    fn into_event(self) -> TimelineEvent {
        let event_type = serde_json::from_value(serde_json::Value::String(self.event_type))
            .unwrap_or(TimelineEventType::StatusChanged);
        let data = serde_json::from_str(&self.data).unwrap_or(serde_json::Value::Null);
        TimelineEvent {
            event_type,
            timestamp: self.timestamp,
            actor: self.actor,
            summary: self.summary,
            data,
        }
    }
}

/// Serialize a `TimelineEventType` to its string representation for DB storage.
pub fn event_type_to_string(et: TimelineEventType) -> Result<String> {
    Ok(serde_json::to_value(et)?
        .as_str()
        .unwrap_or("status_changed")
        .to_string())
}

/// Append an event to a task's timeline.
pub async fn append(pool: &SqlitePool, task_id: i64, event: &TimelineEvent) -> Result<()> {
    let event_type_str = event_type_to_string(event.event_type)?;
    let data_str = serde_json::to_string(&event.data)?;
    sqlx::query(&format!(
        "INSERT INTO timeline_events ({INSERT_COLS}) VALUES (?, ?, ?, ?, ?, ?, ?)"
    ))
    .bind(task_id)
    .bind(&event_type_str)
    .bind(&event.timestamp)
    .bind(&event.actor)
    .bind(&event.summary)
    .bind(&data_str)
    .bind(None::<String>)
    .execute(pool)
    .await?;
    Ok(())
}

/// Atomically append a timeline event AND clear `session_ids.triage` on the
/// task row in a single transaction. Eliminates the crash window that
/// previously required dedupe-key-based replay detection.
///
/// Returns `true` if the event was inserted, `false` if the task row was not
/// found (should not happen in practice).
pub async fn append_and_clear_triage_session(
    pool: &SqlitePool,
    task_id: i64,
    event: &TimelineEvent,
    new_session_ids_json: &str,
) -> Result<bool> {
    let event_type_str = event_type_to_string(event.event_type)?;
    let data_str = serde_json::to_string(&event.data)?;

    let mut tx = pool.begin().await?;

    sqlx::query(&format!(
        "INSERT INTO timeline_events ({INSERT_COLS}) VALUES (?, ?, ?, ?, ?, ?, ?)"
    ))
    .bind(task_id)
    .bind(&event_type_str)
    .bind(&event.timestamp)
    .bind(&event.actor)
    .bind(&event.summary)
    .bind(&data_str)
    .bind(None::<String>)
    .execute(&mut *tx)
    .await?;

    let result = sqlx::query("UPDATE tasks SET session_ids = ?, rev = rev + 1 WHERE id = ?")
        .bind(new_session_ids_json)
        .bind(task_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    Ok(result.rows_affected() > 0)
}

/// Load all timeline events for a task, ordered chronologically.
pub async fn load(pool: &SqlitePool, task_id: i64) -> Result<Vec<TimelineEvent>> {
    let rows: Vec<TimelineRow> =
        sqlx::query_as("SELECT event_type, timestamp, actor, summary, data FROM timeline_events WHERE task_id = ? ORDER BY timestamp ASC")
            .bind(task_id)
            .fetch_all(pool)
            .await?;
    Ok(rows.into_iter().map(|r| r.into_event()).collect())
}

/// Load the last N timeline events for a task.
pub async fn load_last_n(pool: &SqlitePool, task_id: i64, n: i64) -> Result<Vec<TimelineEvent>> {
    let rows: Vec<TimelineRow> = sqlx::query_as(
        "SELECT event_type, timestamp, actor, summary, data FROM (
            SELECT event_type, timestamp, actor, summary, data FROM timeline_events WHERE task_id = ? ORDER BY timestamp DESC LIMIT ?
         ) ORDER BY timestamp ASC",
    )
    .bind(task_id)
    .bind(n)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|r| r.into_event()).collect())
}

/// Fetch the latest clarifier questions from the most recent ClarifyQuestion event.
/// Returns the raw JSON value of the `questions` field (structured array or legacy string).
pub async fn latest_clarifier_questions(
    pool: &SqlitePool,
    task_id: i64,
) -> Result<Option<serde_json::Value>> {
    let event_type_str = event_type_to_string(TimelineEventType::ClarifyQuestion)?;
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT data FROM timeline_events
         WHERE task_id = ? AND event_type = ?
         ORDER BY timestamp DESC LIMIT 1",
    )
    .bind(task_id)
    .bind(&event_type_str)
    .fetch_optional(pool)
    .await?;
    let questions =
        row.and_then(
            |(data,)| match serde_json::from_str::<serde_json::Value>(&data) {
                Ok(val) => {
                    let q = &val["questions"];
                    if q.is_null() {
                        None
                    } else {
                        Some(q.clone())
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        module = "timeline",
                        task_id,
                        error = %e,
                        "corrupt ClarifyQuestion event data"
                    );
                    None
                }
            },
        );
    Ok(questions)
}

/// Load all auto_merge_triage*, human_reopen, and rework_requested events
/// for a task in chronological order. Used by the captain to derive the
/// triage gate state (cycle open / failures / exhausted) from a single
/// focused query instead of the full timeline.
///
/// Secondary sort on `id` (insertion order) ensures a deterministic order
/// when two events share an RFC 3339 second-precision timestamp.
pub async fn load_triage_gate_events(
    pool: &SqlitePool,
    task_id: i64,
) -> Result<Vec<TimelineEvent>> {
    let rows: Vec<TimelineRow> = sqlx::query_as(
        "SELECT event_type, timestamp, actor, summary, data
         FROM timeline_events
         WHERE task_id = ?
           AND event_type IN (
               'auto_merge_triage',
               'auto_merge_triage_failed',
               'auto_merge_triage_exhausted',
               'human_reopen',
               'rework_requested'
           )
         ORDER BY timestamp ASC, id ASC",
    )
    .bind(task_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|r| r.into_event()).collect())
}
