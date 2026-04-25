//! Timeline event emission via SQLite with bounded retry.
//!
//! Retries transient DB failures (3 attempts, 100ms apart) before giving up.
//! Never blocks the critical path beyond the retry budget (~200ms worst case).

use api_types::TimelineEventPayload;

use crate::TimelineEvent;
use anyhow::{anyhow, Result};
use global_db::lifecycle::{mark_outbox_failed, mark_outbox_processed, LifecycleEffect};
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
#[tracing::instrument(skip_all)]
pub(crate) async fn emit(
    pool: &SqlitePool,
    task_id: i64,
    actor: &str,
    summary: &str,
    data: TimelineEventPayload,
) -> Result<()> {
    let event = TimelineEvent {
        timestamp: global_types::now_rfc3339(),
        actor: actor.to_string(),
        summary: summary.to_string(),
        data,
    };

    let mut last_err = None;
    for attempt in 1..=MAX_ATTEMPTS {
        match enqueue_and_project(pool, task_id, &event, actor).await {
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

    // All retries exhausted — permanent audit trail gap. The retry
    // loop above always populates `last_err` before exit; fall back to a
    // generic cause if somehow it didn't so the audit log still lands.
    let err = last_err
        .unwrap_or_else(|| anyhow!("timeline retry loop exited without capturing an error"));
    tracing::error!(
        module = "timeline",
        task_id = %task_id,
        event_type = %event.data.event_type_str(),
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
#[tracing::instrument(skip_all)]
pub async fn emit_for_task(
    item: &crate::Task,
    summary: &str,
    data: TimelineEventPayload,
    pool: &SqlitePool,
) -> Result<()> {
    emit(pool, item.id, "captain", summary, data).await
}

/// Emit a rate-limited timeline event with computed retry-at time.
///
/// Resolves the remaining cooldown from the task's active session: if the
/// session used a specific credential, the per-credential cooldown is used;
/// otherwise falls back to the ambient (host-login) cooldown.
#[tracing::instrument(skip_all)]
pub async fn emit_rate_limited(item: &crate::Task, pool: &SqlitePool) -> Result<()> {
    let remaining = task_rate_limit_remaining_secs(item, pool).await?;
    let retry_at = time::OffsetDateTime::now_utc() + time::Duration::seconds(remaining as i64);
    let retry_at_str = retry_at
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default();
    emit_for_task(
        item,
        &format!("Rate limited — will retry at {retry_at_str}"),
        TimelineEventPayload::RateLimited {
            remaining_secs: remaining as i64,
            retry_at: retry_at_str,
        },
        pool,
    )
    .await
}

/// Remaining rate-limit seconds scoped to a task's active session.
/// Returns a `Result` so a DB failure in the cooldown lookup propagates
/// up to the caller (this function is the source of truth for the
/// timeline event's `remaining_secs`; a silent 0 would under-report a
/// live rate-limit).
async fn task_rate_limit_remaining_secs(
    item: &crate::Task,
    pool: &SqlitePool,
) -> anyhow::Result<u64> {
    let cred_id = match item.session_ids.worker.as_deref() {
        Some(sid) => sessions_db::get_credential_id(pool, sid)
            .await
            .map_err(|e| anyhow::anyhow!("failed to look up credential for session {sid}: {e}"))?,
        None => None,
    };
    Ok(match cred_id {
        Some(cid) => settings::credentials::cooldown_remaining_secs(pool, cid)
            .await?
            .max(0) as u64,
        None => super::ambient_rate_limit::remaining_secs(),
    })
}

async fn enqueue_and_project(
    pool: &SqlitePool,
    task_id: i64,
    event: &TimelineEvent,
    actor: &str,
) -> Result<()> {
    let event_type = event.data.event_type_str();
    let data_value = crate::io::queries::timeline::data_without_tag(&event.data)?;
    let payload = serde_json::json!({
        "task_id": task_id,
        "event_type": event_type,
        "timestamp": event.timestamp,
        "actor": event.actor,
        "summary": event.summary,
        "data": data_value,
    });
    let transition_id = crate::io::queries::tasks_persist::enqueue_task_effects(
        pool,
        task_id,
        actor,
        Some("timeline_emit"),
        vec![LifecycleEffect {
            effect_kind: "task.timeline.project",
            payload: &payload,
        }],
    )
    .await?
    .ok_or_else(|| anyhow!("timeline emit skipped: task {task_id} not found"))?;
    project_outbox_timeline(pool, transition_id).await
}

async fn project_outbox_timeline(pool: &SqlitePool, transition_id: i64) -> Result<()> {
    let rows = global_db::lifecycle::pending_outbox_for_transition(pool, transition_id).await?;
    for row in rows {
        let process_row = async {
            let payload: serde_json::Value = serde_json::from_str(&row.payload)
                .map_err(|e| anyhow!("decode timeline outbox payload {}: {e}", row.id))?;
            if row.effect_kind != "task.timeline.project" {
                anyhow::bail!(
                    "unsupported timeline emit effect {} for outbox {}",
                    row.effect_kind,
                    row.id
                );
            }

            let task_id = payload["task_id"]
                .as_i64()
                .ok_or_else(|| anyhow!("timeline effect {} missing task_id", row.id))?;
            let event_type = payload["event_type"]
                .as_str()
                .ok_or_else(|| anyhow!("timeline effect {} missing event_type", row.id))?;
            let timestamp = payload["timestamp"]
                .as_str()
                .ok_or_else(|| anyhow!("timeline effect {} missing timestamp", row.id))?;
            let actor = payload["actor"]
                .as_str()
                .ok_or_else(|| anyhow!("timeline effect {} missing actor", row.id))?;
            let summary = payload["summary"]
                .as_str()
                .ok_or_else(|| anyhow!("timeline effect {} missing summary", row.id))?;
            let data = payload
                .get("data")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            let data_str = serde_json::to_string(&data)?;

            crate::io::queries::timeline::insert_or_ignore(
                pool,
                task_id,
                event_type,
                timestamp,
                actor,
                summary,
                &data_str,
                &format!("lifecycle-outbox:{}", row.id),
            )
            .await?;

            mark_outbox_processed(pool, row.id).await?;
            Ok::<(), anyhow::Error>(())
        }
        .await;

        if let Err(err) = process_row {
            mark_outbox_failed(pool, row.id, &format!("timeline projection failed: {err}")).await?;
            return Err(err);
        }
    }
    Ok(())
}
