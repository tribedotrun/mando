use anyhow::Result;
use global_db::lifecycle::{record_transition, LifecycleEffect, LifecycleTransitionRecord};
use serde_json::json;
use sqlx::SqlitePool;

use crate::{service::lifecycle, ItemStatus, Task, TimelineEvent};

/// Resolve the single expected predecessor status string for a given target
/// using the lifecycle transition table. Errors if the target has zero or
/// multiple predecessors that match the caller's constraint — callers that
/// need multi-predecessor support thread the list in explicitly.
fn predecessor_string_for(to: ItemStatus, from: ItemStatus) -> Result<&'static str> {
    anyhow::ensure!(
        lifecycle::valid_predecessors(to).contains(&from),
        "lifecycle table rejects {from:?} -> {to:?}"
    );
    Ok(from.as_str())
}

pub async fn persist_spawn(pool: &SqlitePool, task: &Task) -> Result<()> {
    let metadata = json!({
        "task_id": task.id,
        "worker": task.worker,
        "session_id": task.session_ids.worker,
    });
    let noop = super::noop_effect();
    let applied = super::persist_task_transition(
        pool,
        task,
        "queued",
        "spawn_worker",
        "captain",
        None,
        metadata,
        vec![LifecycleEffect {
            effect_kind: "lifecycle.transition.recorded",
            payload: &noop,
        }],
    )
    .await?;
    anyhow::ensure!(
        applied,
        "persist_spawn: transition rejected for task {}",
        task.id
    );
    Ok(())
}

#[allow(dead_code)]
pub async fn persist_merge_spawn(pool: &SqlitePool, task: &Task) -> Result<()> {
    let metadata = json!({
        "task_id": task.id,
        "merge_session_id": task.session_ids.merge,
    });
    let noop = super::noop_effect();
    let applied = super::persist_task_transition(
        pool,
        task,
        "captain-merging",
        "merge_spawn",
        "captain",
        None,
        metadata,
        vec![LifecycleEffect {
            effect_kind: "lifecycle.transition.recorded",
            payload: &noop,
        }],
    )
    .await?;
    anyhow::ensure!(
        applied,
        "persist_merge_spawn: transition rejected for task {}",
        task.id,
    );
    Ok(())
}

pub async fn persist_clarify_start(pool: &SqlitePool, task: &Task) -> Result<()> {
    let metadata = json!({
        "task_id": task.id,
        "clarifier_session_id": task.session_ids.clarifier,
    });
    let noop = super::noop_effect();
    let applied = super::persist_task_transition(
        pool,
        task,
        "new",
        "start_clarifier",
        "captain",
        None,
        metadata,
        vec![LifecycleEffect {
            effect_kind: "lifecycle.transition.recorded",
            payload: &noop,
        }],
    )
    .await?;
    anyhow::ensure!(
        applied,
        "persist_clarify_start: transition rejected for task {}",
        task.id
    );
    Ok(())
}

/// Commit the `needs-clarification → clarifying` transition before the clarify
/// route runs its inline re-clarification turn. Without this, the row stays at
/// `needs-clarification` while `answer_and_reclarify` tries to write a result,
/// and the subsequent `apply_clarifier_result` → `persist_clarify_result`
/// transition trips the state guard (because it expects to see `clarifying`).
pub async fn persist_resume_clarifier(pool: &SqlitePool, task: &Task) -> Result<()> {
    anyhow::ensure!(
        task.status == ItemStatus::Clarifying,
        "persist_resume_clarifier called with task.status={:?}; expected Clarifying",
        task.status
    );
    let expected_status =
        predecessor_string_for(ItemStatus::Clarifying, ItemStatus::NeedsClarification)?;
    let metadata = json!({
        "task_id": task.id,
        "clarifier_session_id": task.session_ids.clarifier,
    });
    let noop = super::noop_effect();
    let applied = super::persist_task_transition(
        pool,
        task,
        expected_status,
        "resume_clarifier",
        "http",
        None,
        metadata,
        vec![LifecycleEffect {
            effect_kind: "lifecycle.transition.recorded",
            payload: &noop,
        }],
    )
    .await?;
    anyhow::ensure!(
        applied,
        "persist_resume_clarifier: transition rejected for task {}",
        task.id
    );
    Ok(())
}

pub async fn persist_clarify_result(pool: &SqlitePool, task: &Task) -> Result<()> {
    // All legal post-clarifier targets share the same predecessor: Clarifying.
    // Derive the check from the transition table so a new edge landing in
    // `lifecycle::infer_transition_command` keeps this function correct
    // without an extra match arm.
    let expected_status =
        predecessor_string_for(task.status, ItemStatus::Clarifying).map_err(|e| {
            tracing::warn!(
                module = "captain-io-queries-tasks_persist-api",
                task_id = task.id,
                status = task.status.as_str(),
                error = %e,
                "persist_clarify_result called with unexpected target status"
            );
            e
        })?;
    let metadata = json!({
        "task_id": task.id,
        "target_status": task.status.as_str(),
        "clarifier_session_id": task.session_ids.clarifier,
    });
    let noop = super::noop_effect();

    // Open an outer tx so the task transition AND the clarifier session's
    // `result_applied_at` marker land together. If the marker doesn't land
    // atomically with the task transition, a subsequent NC→Clarifying
    // re-entry (user answers a follow-up) would let `tick_clarify_poll`
    // re-apply this same already-consumed stream.
    let mut tx = pool.begin().await?;
    let transition_id = super::persist_task_transition_in_tx(
        &mut tx,
        task,
        expected_status,
        "apply_clarifier_result",
        "captain",
        None,
        &metadata,
        &[LifecycleEffect {
            effect_kind: "lifecycle.transition.recorded",
            payload: &noop,
        }],
    )
    .await?;
    anyhow::ensure!(
        transition_id.is_some(),
        "persist_clarify_result: transition rejected for task {}",
        task.id
    );

    // Every successful clarifier turn lands a session id via
    // `apply_clarifier_result` before this function is called, so a
    // missing id here indicates an upstream bug (the in-memory task
    // state is out of sync with the CC run that produced this result).
    // Debug-assert so tests catch the regression; in release we still
    // commit the transition but log because the fallback is: the
    // `result_applied_at` marker is NOT written, which re-opens the
    // PR #887 hazard where `tick_clarify_poll` could re-apply a stale
    // stream on the next `NeedsClarification -> Clarifying` entry.
    debug_assert!(
        task.session_ids.clarifier.is_some(),
        "persist_clarify_result called without a clarifier session id on task {}",
        task.id
    );
    if let Some(ref sid) = task.session_ids.clarifier {
        sessions_db::mark_session_result_applied_in_tx(&mut tx, sid).await?;
    } else {
        tracing::warn!(
            module = "captain-io-queries-tasks_persist-api",
            task_id = task.id,
            "persist_clarify_result: no clarifier session id to mark applied — \
             a subsequent NC→Clarifying re-entry could let tick_clarify_poll \
             re-apply a stale stream. Investigate the call site."
        );
    }
    tx.commit().await?;
    Ok(())
}

pub async fn persist_status_transition(
    pool: &SqlitePool,
    task: &Task,
    expected_status: &str,
    event: &TimelineEvent,
) -> Result<bool> {
    let command = event.data.event_type_str();
    persist_status_transition_with_command(pool, task, expected_status, command, event).await
}

pub async fn persist_status_transition_with_command(
    pool: &SqlitePool,
    task: &Task,
    expected_status: &str,
    command: &str,
    event: &TimelineEvent,
) -> Result<bool> {
    persist_status_transition_with_command_and_effects(
        pool,
        task,
        expected_status,
        command,
        event,
        Vec::new(),
    )
    .await
}

pub async fn persist_status_transition_with_command_and_effects(
    pool: &SqlitePool,
    task: &Task,
    expected_status: &str,
    command: &str,
    event: &TimelineEvent,
    extra_effects: Vec<LifecycleEffect<'_>>,
) -> Result<bool> {
    let event_type = event.data.event_type_str();
    let metadata = json!({
        "task_id": task.id,
        "event_type": event_type,
        "summary": event.summary,
        "actor": event.actor,
    });
    let effect_payload = super::timeline_effect(task.id, event_type, event)?;
    let mut effects = Vec::with_capacity(extra_effects.len() + 1);
    effects.push(LifecycleEffect {
        effect_kind: "task.timeline.project",
        payload: &effect_payload,
    });
    effects.extend(extra_effects);
    super::persist_task_transition(
        pool,
        task,
        expected_status,
        command,
        &event.actor,
        None,
        metadata,
        effects,
    )
    .await
}

pub async fn enqueue_task_effects(
    pool: &SqlitePool,
    task_id: i64,
    actor: &str,
    cause: Option<&str>,
    effects: Vec<LifecycleEffect<'_>>,
) -> Result<Option<i64>> {
    let mut tx = pool.begin().await?;
    let Some((current_status, current_rev)) = super::load_status_and_rev(&mut tx, task_id).await?
    else {
        tx.rollback().await?;
        return Ok(None);
    };
    let aggregate_id = task_id.to_string();
    let metadata = json!({
        "task_id": task_id,
        "effect_count": effects.len(),
    });
    let transition_id = record_transition(
        &mut tx,
        &LifecycleTransitionRecord {
            aggregate_type: "task",
            aggregate_id: &aggregate_id,
            command: "dispatch_effects",
            from_state: Some(current_status.as_str()),
            to_state: current_status.as_str(),
            actor,
            cause,
            metadata: &metadata,
            rev_before: current_rev,
            rev_after: current_rev,
            idempotency_key: None,
        },
        &effects,
    )
    .await?;
    tx.commit().await?;
    Ok(Some(transition_id))
}

pub async fn revert_orphaned_planning(pool: &SqlitePool) -> Result<u64> {
    let mut tx = pool.begin().await?;
    let now = global_types::now_rfc3339();
    let command = crate::service::lifecycle::infer_transition_command(
        crate::ItemStatus::InProgress,
        crate::ItemStatus::Queued,
        true,
    )?;
    let mut transition_ids = Vec::new();
    let tasks: Vec<(i64, i64)> =
        sqlx::query_as("SELECT id, rev FROM tasks WHERE status = 'in-progress' AND planning = 1")
            .fetch_all(&mut *tx)
            .await?;
    for (task_id, rev) in &tasks {
        let result = sqlx::query(
            "UPDATE tasks
             SET status = 'queued', worker = NULL, session_ids = '{}', worker_started_at = NULL,
                 last_activity_at = ?, rev = rev + 1
             WHERE id = ? AND rev = ?",
        )
        .bind(&now)
        .bind(task_id)
        .bind(rev)
        .execute(&mut *tx)
        .await?;
        if result.rows_affected() == 0 {
            continue;
        }
        let metadata = json!({"task_id": task_id, "reason": "orphaned_planning_recovery"});
        let noop = super::noop_effect();
        let aggregate_id = task_id.to_string();
        let transition_id = record_transition(
            &mut tx,
            &LifecycleTransitionRecord {
                aggregate_type: "task",
                aggregate_id: &aggregate_id,
                command,
                from_state: Some("in-progress"),
                to_state: "queued",
                actor: "captain",
                cause: Some("daemon_restart"),
                metadata: &metadata,
                rev_before: *rev,
                rev_after: *rev + 1,
                idempotency_key: None,
            },
            &[LifecycleEffect {
                effect_kind: "lifecycle.transition.recorded",
                payload: &noop,
            }],
        )
        .await?;
        transition_ids.push(transition_id);
    }
    tx.commit().await?;
    Ok(transition_ids.len() as u64)
}

#[cfg(test)]
mod tests {
    use sqlx::Row;

    use super::*;
    use crate::{io::queries::tasks, ItemStatus};

    async fn test_pool() -> SqlitePool {
        let db = global_db::Db::open_in_memory().await.unwrap();
        settings::projects::upsert(db.pool(), "test", "", None)
            .await
            .unwrap();
        db.pool().clone()
    }

    fn test_task(title: &str) -> Task {
        let mut task = Task::new(title);
        task.project_id = 1;
        task.project = "test".into();
        task
    }

    #[tokio::test]
    async fn persist_merge_spawn_records_lifecycle_transition() {
        let pool = test_pool().await;
        let mut task = test_task("merge me");
        task.status = ItemStatus::CaptainMerging;
        task.last_activity_at = Some(global_types::now_rfc3339());
        let id = tasks::insert_task(&pool, &task).await.unwrap();

        let mut persisted = tasks::find_by_id(&pool, id).await.unwrap().unwrap();
        persisted.session_ids.merge = Some("merge-session-1".into());
        persisted.last_activity_at = Some(global_types::now_rfc3339());

        persist_merge_spawn(&pool, &persisted).await.unwrap();

        let after = tasks::find_by_id(&pool, id).await.unwrap().unwrap();
        assert_eq!(after.status, ItemStatus::CaptainMerging);
        assert_eq!(after.rev, persisted.rev + 1);
        assert_eq!(after.session_ids.merge.as_deref(), Some("merge-session-1"));

        let transition = sqlx::query(
            "SELECT command, from_state, to_state, rev_before, rev_after
             FROM lifecycle_transitions
             WHERE aggregate_type = 'task' AND aggregate_id = ?
             ORDER BY id DESC
             LIMIT 1",
        )
        .bind(id.to_string())
        .fetch_one(&pool)
        .await
        .unwrap();

        assert_eq!(transition.get::<String, _>("command"), "merge_spawn");
        assert_eq!(
            transition.get::<Option<String>, _>("from_state").as_deref(),
            Some("captain-merging")
        );
        assert_eq!(transition.get::<String, _>("to_state"), "captain-merging");
        assert_eq!(transition.get::<i64, _>("rev_before"), persisted.rev);
        assert_eq!(transition.get::<i64, _>("rev_after"), persisted.rev + 1);
    }
}
