//! Timeline event emission via SQLite with bounded retry.
//!
//! Retries transient DB failures (3 attempts, 100ms apart) before giving up.
//! Never blocks the critical path beyond the retry budget (~200ms worst case).

use anyhow::{anyhow, Result};
use mando_types::timeline::{TimelineEvent, TimelineEventType};
use sqlx::SqlitePool;

const MAX_ATTEMPTS: u32 = 3;
const RETRY_DELAY_MS: u64 = 100;

/// Emit a timeline event with bounded retry.
///
/// Retries up to 3 times (100ms between attempts) to survive transient DB
/// hiccups. If all attempts fail, logs at ERROR and returns an error so
/// callers can escalate, persist to a fallback, or surface the gap in a
/// tick alert. Callers that legitimately treat the timeline as best-effort
/// can ignore the result with `let _ =` — but the default is to handle it.
pub(crate) async fn emit(
    pool: &SqlitePool,
    task_id: i64,
    event_type: TimelineEventType,
    actor: &str,
    summary: &str,
    data: serde_json::Value,
) -> Result<()> {
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
            Ok(()) => return Ok(()),
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
    Err(anyhow!(
        "timeline event lost after {MAX_ATTEMPTS} attempts: {err}"
    ))
}

/// Emit a timeline event for a task.
pub async fn emit_for_task(
    item: &mando_types::Task,
    event_type: TimelineEventType,
    summary: &str,
    data: serde_json::Value,
    pool: &SqlitePool,
) -> Result<()> {
    emit(pool, item.id, event_type, "captain", summary, data).await
}

/// Emit a rate-limited timeline event with computed retry-at time.
///
/// Resolves the remaining cooldown from the task's active session: if the
/// session used a specific credential, the per-credential cooldown is used;
/// otherwise falls back to the ambient (host-login) cooldown.
pub async fn emit_rate_limited(item: &mando_types::Task, pool: &SqlitePool) -> Result<()> {
    let remaining = task_rate_limit_remaining_secs(item, pool).await;
    let retry_at = time::OffsetDateTime::now_utc() + time::Duration::seconds(remaining as i64);
    let retry_at_str = retry_at
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default();
    emit_for_task(
        item,
        TimelineEventType::RateLimited,
        &format!("Rate limited — will retry at {retry_at_str}"),
        serde_json::json!({ "remaining_secs": remaining, "retry_at": retry_at_str }),
        pool,
    )
    .await
}

/// Remaining rate-limit seconds scoped to a task's active session.
async fn task_rate_limit_remaining_secs(item: &mando_types::Task, pool: &SqlitePool) -> u64 {
    let cred_id = match item.session_ids.worker.as_deref() {
        Some(sid) => mando_db::queries::sessions::get_credential_id(pool, sid)
            .await
            .unwrap_or(None),
        None => None,
    };
    match cred_id {
        Some(cid) => mando_db::queries::credentials::cooldown_remaining_secs(pool, cid)
            .await
            .max(0) as u64,
        None => super::ambient_rate_limit::remaining_secs(),
    }
}
