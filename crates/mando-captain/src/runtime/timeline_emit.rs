//! Timeline event emission via SQLite with bounded retry.
//!
//! Retries transient DB failures (3 attempts, 100ms apart) before giving up.
//! Never blocks the critical path beyond the retry budget (~200ms worst case).

use mando_types::timeline::{TimelineEvent, TimelineEventType};
use sqlx::SqlitePool;

const MAX_ATTEMPTS: u32 = 3;
const RETRY_DELAY_MS: u64 = 100;

/// Emit a timeline event with bounded retry.
///
/// Retries up to 3 times (100ms between attempts) to survive transient DB
/// hiccups. If all attempts fail, logs at ERROR with enough context to
/// identify the gap. Never blocks the caller beyond ~200ms.
pub(crate) async fn emit(
    pool: &SqlitePool,
    task_id: i64,
    event_type: TimelineEventType,
    actor: &str,
    summary: &str,
    data: serde_json::Value,
) {
    let event = TimelineEvent {
        event_type,
        timestamp: mando_types::now_rfc3339(),
        actor: actor.to_string(),
        summary: summary.to_string(),
        data,
    };

    let mut last_err = None;
    for attempt in 1..=MAX_ATTEMPTS {
        match mando_db::queries::timeline::append(pool, task_id, &event).await {
            Ok(()) => return,
            Err(e) => {
                if attempt < MAX_ATTEMPTS {
                    tracing::warn!(
                        module = "timeline",
                        task_id = %task_id,
                        attempt,
                        error = %e,
                        "timeline persist failed, retrying"
                    );
                    tokio::time::sleep(std::time::Duration::from_millis(RETRY_DELAY_MS)).await;
                }
                last_err = Some(e);
            }
        }
    }

    // All retries exhausted — permanent audit trail gap.
    let err = last_err.expect("loop ran at least once");
    tracing::error!(
        module = "timeline",
        task_id = %task_id,
        event_type = ?event.event_type,
        actor = %event.actor,
        summary = %event.summary,
        attempts = MAX_ATTEMPTS,
        error = %err,
        "timeline event lost after {MAX_ATTEMPTS} attempts — audit trail gap"
    );
}

/// Emit a timeline event for a task.
pub async fn emit_for_task(
    item: &mando_types::Task,
    event_type: TimelineEventType,
    summary: &str,
    data: serde_json::Value,
    pool: &SqlitePool,
) {
    emit(pool, item.id, event_type, "captain", summary, data).await;
}
