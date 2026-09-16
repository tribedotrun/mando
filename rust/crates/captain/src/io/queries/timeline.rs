//! Timeline event queries.

use anyhow::{anyhow, Result};
use sqlx::SqlitePool;

use crate::{TimelineEvent, TimelineEventPayload};

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
    /// Decode a row into a typed event.
    ///
    /// The `event_type` is a separate SQL column; the tagged-enum payload
    /// needs its discriminator in the JSON, so we inject the column value
    /// into the `data` object before handing it to serde. A row whose
    /// stored JSON doesn't round-trip cleanly into any
    /// [`TimelineEventPayload`] variant is a hard error — the decoder
    /// rejects unknown fields, which is exactly the drift guardrail PR
    /// #855 intended to install.
    fn into_event(self) -> Result<TimelineEvent> {
        let raw_data = self.data.clone();
        let mut value: serde_json::Value = if self.data.trim().is_empty() {
            serde_json::Value::Object(Default::default())
        } else {
            serde_json::from_str(&self.data)
                .map_err(|e| anyhow!("timeline row data is not valid JSON: {e}"))?
        };
        let Some(obj) = value.as_object_mut() else {
            return Err(anyhow!(
                "timeline row data must be a JSON object, got {value:?}"
            ));
        };
        obj.insert(
            "event_type".to_string(),
            serde_json::Value::String(self.event_type.clone()),
        );
        let data: TimelineEventPayload = serde_json::from_value(value).map_err(|e| {
            anyhow!(
                "timeline row event_type={} raw_data={raw_data} has payload that does not match the wire contract: {e}",
                self.event_type
            )
        })?;
        Ok(TimelineEvent {
            timestamp: self.timestamp,
            actor: self.actor,
            summary: self.summary,
            data,
        })
    }
}

/// Insert a raw timeline event row. `INSERT OR IGNORE` on the dedupe key
/// so retried outbox rows don't double-project into the timeline.
#[allow(clippy::too_many_arguments)]
pub async fn insert_or_ignore(
    pool: &SqlitePool,
    task_id: i64,
    event_type: &str,
    timestamp: &str,
    actor: &str,
    summary: &str,
    data_str: &str,
    dedupe_key: &str,
) -> Result<()> {
    sqlx::query(&format!(
        "INSERT OR IGNORE INTO timeline_events ({INSERT_COLS}) VALUES (?, ?, ?, ?, ?, ?, ?)"
    ))
    .bind(task_id)
    .bind(event_type)
    .bind(timestamp)
    .bind(actor)
    .bind(summary)
    .bind(data_str)
    .bind(dedupe_key)
    .execute(pool)
    .await?;
    Ok(())
}

/// Serialize a payload to JSON with the `event_type` tag stripped (the tag
/// lives in the separate SQL column; duplicating it inside the blob would
/// make round-tripping fragile). Thin crate-local wrapper around
/// `TimelineEventPayload::data_without_tag` so callers inside captain can
/// compose with `anyhow::Result`.
pub(crate) fn data_without_tag(payload: &TimelineEventPayload) -> Result<serde_json::Value> {
    Ok(payload.data_without_tag()?)
}

/// Load all timeline events for a task, ordered chronologically.
pub async fn load(pool: &SqlitePool, task_id: i64) -> Result<Vec<TimelineEvent>> {
    let rows: Vec<TimelineRow> =
        sqlx::query_as("SELECT event_type, timestamp, actor, summary, data FROM timeline_events WHERE task_id = ? ORDER BY timestamp ASC")
            .bind(task_id)
            .fetch_all(pool)
            .await?;
    rows.into_iter().map(|r| r.into_event()).collect()
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
    rows.into_iter().map(|r| r.into_event()).collect()
}

/// Fetch the latest clarifier questions from the most recent ClarifyQuestion event.
pub async fn latest_clarifier_questions(
    pool: &SqlitePool,
    task_id: i64,
) -> Result<Option<Vec<api_types::ClarifierQuestionPayload>>> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT data FROM timeline_events
         WHERE task_id = ? AND event_type = 'clarify_question'
         ORDER BY timestamp DESC LIMIT 1",
    )
    .bind(task_id)
    .fetch_optional(pool)
    .await?;
    let Some((data,)) = row else {
        return Ok(None);
    };
    let parsed: serde_json::Value = match serde_json::from_str(&data) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(
                module = "timeline",
                task_id,
                error = %e,
                "corrupt ClarifyQuestion event data"
            );
            return Ok(None);
        }
    };
    let Some(questions) = parsed.get("questions") else {
        return Ok(None);
    };
    if questions.is_null() {
        return Ok(None);
    }
    let parsed_questions: Vec<api_types::ClarifierQuestionPayload> =
        serde_json::from_value(questions.clone())
            .map_err(|e| anyhow!("ClarifyQuestion event questions field did not decode: {e}"))?;
    Ok(Some(parsed_questions))
}

/// Load the most recent `awaiting_review` event for a task, if any.
///
/// Emitted on every ship verdict. The mergeability tick reads the
/// `confidence` field on the latest verdict to decide whether to auto-merge
/// a mergeable PR; missing confidence or mid/low confidence = safe-degrade
/// to human review.
pub async fn load_latest_ship_verdict(
    pool: &SqlitePool,
    task_id: i64,
) -> Result<Option<TimelineEvent>> {
    let row: Option<TimelineRow> = sqlx::query_as(
        "SELECT event_type, timestamp, actor, summary, data
         FROM timeline_events
         WHERE task_id = ? AND event_type = 'awaiting_review'
         ORDER BY timestamp DESC, id DESC
         LIMIT 1",
    )
    .bind(task_id)
    .fetch_optional(pool)
    .await?;
    row.map(|r| r.into_event()).transpose()
}
