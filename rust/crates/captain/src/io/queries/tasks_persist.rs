//! Immediate-persist helpers for task fields.
//!
//! Each function writes a narrow set of columns directly to SQLite so the DB
//! reflects in-flight work even if captain crashes before tick-end merge.

use anyhow::{Context, Result};
use global_db::lifecycle::{record_transition, LifecycleEffect, LifecycleTransitionRecord};
use serde_json::{json, Value};
use sqlx::SqlitePool;

use crate::{Task, TimelineEvent, TimelineEventPayload};

use super::tasks::{bind_task_write_fields, update_set_clause};

mod api;

pub use api::{
    enqueue_task_effects, persist_clarify_result, persist_clarify_start, persist_resume_clarifier,
    persist_spawn, persist_status_transition, persist_status_transition_with_command,
    persist_status_transition_with_command_and_effects, revert_orphaned_planning,
};

async fn load_status_and_rev(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    task_id: i64,
) -> Result<Option<(String, i64)>> {
    sqlx::query_as::<_, (String, i64)>("SELECT status, rev FROM tasks WHERE id = ?")
        .bind(task_id)
        .fetch_optional(&mut **tx)
        .await
        .context("load current task status and rev")
}

fn noop_effect() -> Value {
    json!({ "kind": "transition_recorded" })
}

fn timeline_effect(task_id: i64, event_type: &str, event: &TimelineEvent) -> Result<Value> {
    let data = super::timeline::data_without_tag(&event.data)?;
    Ok(json!({
        "task_id": task_id,
        "event_type": event_type,
        "timestamp": event.timestamp,
        "actor": event.actor,
        "summary": event.summary,
        "data": data,
    }))
}

pub(crate) fn task_bus_effect(task_id: i64, action: &str) -> Value {
    json!({
        "task_id": task_id,
        "action": action,
    })
}

pub(crate) fn workbench_touch_effect(workbench_id: i64) -> Value {
    json!({
        "workbench_id": workbench_id,
    })
}

pub(crate) async fn record_task_created_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    task: &Task,
    actor: &str,
    source: Option<&str>,
) -> Result<i64> {
    let event = TimelineEvent {
        timestamp: global_types::now_rfc3339(),
        actor: actor.to_string(),
        summary: format!("Item created: {}", task.title),
        data: TimelineEventPayload::Created {
            source: source.unwrap_or("unknown").to_string(),
        },
    };
    let event_type = event.data.event_type_str();
    let metadata = json!({
        "task_id": task.id,
        "event_type": event_type,
        "summary": event.summary,
        "actor": event.actor,
    });
    let effect_payload = timeline_effect(task.id, event_type, &event)?;
    let aggregate_id = task.id.to_string();
    record_transition(
        tx,
        &LifecycleTransitionRecord {
            aggregate_type: "task",
            aggregate_id: &aggregate_id,
            command: "create",
            from_state: None,
            to_state: task.status.as_str(),
            actor,
            cause: source,
            metadata: &metadata,
            rev_before: 0,
            rev_after: task.rev,
            idempotency_key: None,
        },
        &[LifecycleEffect {
            effect_kind: "task.timeline.project",
            payload: &effect_payload,
        }],
    )
    .await
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn persist_task_transition_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    task: &Task,
    expected_status: &str,
    command: &str,
    actor: &str,
    cause: Option<&str>,
    metadata: &Value,
    effects: &[LifecycleEffect<'_>],
) -> Result<Option<i64>> {
    let Some((current_status, current_rev)) = load_status_and_rev(tx, task.id).await? else {
        return Ok(None);
    };

    if current_status != expected_status {
        tracing::info!(
            module = "task-store",
            task_id = task.id,
            expected_status,
            current_status,
            new_status = task.status.as_str(),
            command,
            "task transition skipped by state guard"
        );
        return Ok(None);
    }

    let from_status: crate::ItemStatus = current_status.parse().map_err(|e: String| {
        anyhow::anyhow!("invalid persisted task status {current_status}: {e}")
    })?;
    let _ = crate::service::lifecycle::infer_transition_command(
        from_status,
        task.status,
        task.planning,
    )?;

    let set_clause = update_set_clause();
    let result = bind_task_write_fields(
        sqlx::query(&format!(
            "UPDATE tasks SET {set_clause} WHERE id = ? AND status = ? AND rev = ?"
        )),
        task,
    )
    .bind(task.id)
    .bind(expected_status)
    .bind(current_rev)
    .execute(&mut **tx)
    .await?;

    if result.rows_affected() == 0 {
        tracing::info!(
            module = "task-store",
            task_id = task.id,
            expected_status,
            current_rev,
            new_status = task.status.as_str(),
            command,
            "task transition skipped by revision guard"
        );
        return Ok(None);
    }

    let aggregate_id = task.id.to_string();
    let transition_id = record_transition(
        tx,
        &LifecycleTransitionRecord {
            aggregate_type: "task",
            aggregate_id: &aggregate_id,
            command,
            from_state: Some(expected_status),
            to_state: task.status.as_str(),
            actor,
            cause,
            metadata,
            rev_before: current_rev,
            rev_after: current_rev + 1,
            idempotency_key: None,
        },
        effects,
    )
    .await?;
    Ok(Some(transition_id))
}

#[allow(clippy::too_many_arguments)]
async fn persist_task_transition(
    pool: &SqlitePool,
    task: &Task,
    expected_status: &str,
    command: &str,
    actor: &str,
    cause: Option<&str>,
    metadata: Value,
    effects: Vec<LifecycleEffect<'_>>,
) -> Result<bool> {
    let mut tx = pool.begin().await?;
    let transition_id = persist_task_transition_in_tx(
        &mut tx,
        task,
        expected_status,
        command,
        actor,
        cause,
        &metadata,
        &effects,
    )
    .await?;
    tx.commit().await?;
    if let Some(transition_id) = transition_id {
        tracing::info!(
            module = "task-store",
            task_id = task.id,
            command,
            from_status = expected_status,
            to_status = task.status.as_str(),
            transition_id,
            "task transition committed"
        );
        Ok(true)
    } else {
        Ok(false)
    }
}
