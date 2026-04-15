//! Timeline event queries — replaces file-based timeline store.

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
///
/// Generates a unique dedupe key from `{task_id}-{event_type}-{timestamp}`.
/// For events that need stronger idempotency (status transitions), use
/// `persist_status_transition` instead which includes a status guard.
pub async fn append(pool: &SqlitePool, task_id: i64, event: &TimelineEvent) -> Result<()> {
    let event_type_str = event_type_to_string(event.event_type)?;
    let data_str = serde_json::to_string(&event.data)?;
    let dedupe_key = format!("{}-{}-{}", task_id, event_type_str, event.timestamp);
    sqlx::query(&format!(
        "INSERT INTO timeline_events ({INSERT_COLS}) VALUES (?, ?, ?, ?, ?, ?, ?)"
    ))
    .bind(task_id)
    .bind(&event_type_str)
    .bind(&event.timestamp)
    .bind(&event.actor)
    .bind(&event.summary)
    .bind(&data_str)
    .bind(&dedupe_key)
    .execute(pool)
    .await?;
    Ok(())
}

/// Outcome of `append_with_dedupe_key`. Callers that drive an idempotent
/// state machine (e.g. the captain's auto-merge triage) need to distinguish
/// "I just wrote the event" from "the row was already there" — the latter
/// happens when the daemon crashes after writing the event but before the
/// in-memory state machine persists its post-write side-effects, so on the
/// next tick the same event is re-attempted and trips the UNIQUE index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppendResult {
    /// Row was inserted by this call.
    Inserted,
    /// Row already existed (UNIQUE conflict on dedupe_key); the caller's
    /// post-write side-effects have already happened on a prior call.
    AlreadyExists,
}

/// SQLite UNIQUE-constraint violation code (`SQLITE_CONSTRAINT_UNIQUE`).
const SQLITE_UNIQUE_VIOLATION_CODE: &str = "2067";

/// Append an event with a caller-provided dedupe key. Returns
/// `AppendResult::AlreadyExists` instead of an error when the dedupe key
/// is already present, so callers can treat repeated writes as a no-op.
pub async fn append_with_dedupe_key(
    pool: &SqlitePool,
    task_id: i64,
    event: &TimelineEvent,
    dedupe_key: &str,
) -> Result<AppendResult> {
    let event_type_str = event_type_to_string(event.event_type)?;
    let data_str = serde_json::to_string(&event.data)?;
    let result = sqlx::query(&format!(
        "INSERT INTO timeline_events ({INSERT_COLS}) VALUES (?, ?, ?, ?, ?, ?, ?)"
    ))
    .bind(task_id)
    .bind(&event_type_str)
    .bind(&event.timestamp)
    .bind(&event.actor)
    .bind(&event.summary)
    .bind(&data_str)
    .bind(dedupe_key)
    .execute(pool)
    .await;
    match result {
        Ok(_) => Ok(AppendResult::Inserted),
        Err(sqlx::Error::Database(db_err))
            if db_err.code().as_deref() == Some(SQLITE_UNIQUE_VIOLATION_CODE) =>
        {
            Ok(AppendResult::AlreadyExists)
        }
        Err(e) => Err(e.into()),
    }
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

/// Check if a backfill marker exists for a task.
///
/// Uses the UNIQUE dedupe_key index for an O(1) lookup instead of scanning
/// the JSON `data` column. The marker's dedupe_key is always
/// `"{task_id}-status_changed-backfill-"` (empty timestamp).
pub async fn has_backfill_marker(pool: &SqlitePool, task_id: i64) -> Result<bool> {
    let dedupe_key = format!("{task_id}-status_changed-backfill-");
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM timeline_events WHERE dedupe_key = ?)")
            .bind(&dedupe_key)
            .fetch_one(pool)
            .await?;
    Ok(exists)
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

/// Build the dedupe key for an auto_merge_triage success timeline event.
///
/// Anchored to `session_id` (not the wall clock) so that a daemon restart
/// between event-write and `session_ids.triage = None` persistence cannot
/// produce duplicate verdicts: each triage CC session can complete at most
/// once, and the UNIQUE constraint enforces that.
pub fn auto_merge_triage_dedupe_key(task_id: i64, session_id: &str) -> String {
    format!("{task_id}-auto_merge_triage-{session_id}")
}

/// Build the dedupe key for an auto_merge_triage_failed timeline event.
///
/// Anchored to `session_id` for restart-idempotency: each spawned triage
/// session can fail at most once. Without this, a crash between event-write
/// and session-id-clear would let the next tick re-emit the same failure
/// with a new timestamp, silently inflating the failure count and tripping
/// premature exhaustion.
pub fn auto_merge_triage_failed_dedupe_key(task_id: i64, session_id: &str) -> String {
    format!("{task_id}-auto_merge_triage_failed-{session_id}")
}

/// Build the dedupe key for an auto_merge_triage_exhausted timeline event.
///
/// Anchored to (`reopen_seq`, `last_failure_at`) so each cycle's exhaustion
/// gets a distinct key. `reopen_seq` alone isn't sufficient because
/// `ReworkRequested` opens a fresh triage cycle (`derive_gate_state` resets
/// `failures_in_cycle` to 0) without incrementing `reopen_seq` — two
/// exhaustions in different cycles separated by a rework would otherwise
/// collide on the UNIQUE index. The caller passes the timestamp of the
/// final failure event in the cycle, which is naturally distinct per cycle.
pub fn auto_merge_triage_exhausted_dedupe_key(
    task_id: i64,
    reopen_seq: i64,
    last_failure_at: &str,
) -> String {
    format!("{task_id}-auto_merge_triage_exhausted-seq{reopen_seq}-{last_failure_at}")
}

/// Load all auto_merge_triage*, human_reopen, and rework_requested events
/// for a task in chronological order. Used by the captain to derive the
/// triage gate state (cycle open / failures / exhausted) from a single
/// focused query instead of the full timeline.
///
/// Secondary sort on `id` (insertion order) ensures a deterministic order
/// when two events share an RFC 3339 second-precision timestamp — important
/// because the gate state derivation is order-sensitive (a HumanReopen that
/// arrives in the same second as a tail AutoMergeTriageFailed must not
/// reset before the failure is counted, or vice versa).
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

/// Bulk insert events (for backfill).
pub async fn bulk_insert(pool: &SqlitePool, task_id: i64, events: &[TimelineEvent]) -> Result<()> {
    let mut tx = pool.begin().await?;
    for event in events {
        let event_type_str = event_type_to_string(event.event_type)?;
        let data_str = serde_json::to_string(&event.data)?;
        let dedupe_key = format!(
            "{}-{}-backfill-{}",
            task_id, event_type_str, event.timestamp
        );
        sqlx::query(
            "INSERT INTO timeline_events \
             (task_id, event_type, timestamp, actor, summary, data, dedupe_key) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )
        .bind(task_id)
        .bind(&event_type_str)
        .bind(&event.timestamp)
        .bind(&event.actor)
        .bind(&event.summary)
        .bind(&data_str)
        .bind(&dedupe_key)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await?;
    Ok(())
}
