//! Fire-and-forget timeline event emission via SQLite.
//!
//! Never blocks the critical path. Logs a warning on error.

use mando_types::timeline::{TimelineEvent, TimelineEventType};
use sqlx::SqlitePool;

/// Emit a timeline event — fire-and-forget.
///
/// Logs a warning on error. Never blocks the caller.
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
        timestamp: now_iso(),
        actor: actor.to_string(),
        summary: summary.to_string(),
        data,
    };

    if let Err(e) = mando_db::queries::timeline::append(pool, task_id, &event).await {
        tracing::warn!(
            module = "timeline",
            task_id = %task_id,
            error = %e,
            "failed to persist timeline event — audit trail gap"
        );
    }
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

fn now_iso() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Basic ISO 8601 without external deps.
    let days = secs / 86400;
    let remaining = secs % 86400;
    let hours = remaining / 3600;
    let minutes = (remaining % 3600) / 60;
    let seconds = remaining % 60;

    let (year, month, day) = days_to_ymd(days);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hours, minutes, seconds
    )
}

fn days_to_ymd(mut days: u64) -> (u64, u64, u64) {
    let mut year = 1970;
    loop {
        let days_in_year = if is_leap(year) { 366 } else { 365 };
        if days < days_in_year {
            break;
        }
        days -= days_in_year;
        year += 1;
    }
    let leap = is_leap(year);
    let month_days = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut month = 1u64;
    for &md in &month_days {
        if days < md {
            break;
        }
        days -= md;
        month += 1;
    }
    (year, month, days + 1)
}

fn is_leap(year: u64) -> bool {
    mando_shared::cron::is_leap(year as i32)
}
