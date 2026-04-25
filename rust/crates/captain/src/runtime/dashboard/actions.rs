use anyhow::Result;
use api_types::MergeResponse;

use crate::{ItemStatus, TimelineEvent, TimelineEventPayload, UpdateTaskInput};

use super::{apply_manual_command, update_task, TaskLifecycleCommand, TaskStore};

#[tracing::instrument(skip_all)]
pub async fn validate_rate_limited_task(store: &TaskStore, id: i64) -> Result<()> {
    let task = store
        .find_by_id(id)
        .await?
        .ok_or(crate::TaskActionError::NotFound(id))?;
    anyhow::ensure!(
        matches!(
            task.status,
            ItemStatus::CaptainReviewing | ItemStatus::CaptainMerging | ItemStatus::Clarifying
        ),
        "resume requires captain-reviewing/captain-merging/clarifying, got {:?}",
        task.status
    );

    let pool = store.pool();
    let ambient_active = super::super::ambient_rate_limit::is_active();
    let credential_blocking =
        settings::credentials::earliest_cooldown_remaining_secs(pool).await? > 0;
    anyhow::ensure!(
        ambient_active || credential_blocking,
        "rate-limit cooldown is not active"
    );
    if ambient_active {
        super::super::ambient_rate_limit::clear();
    }
    if credential_blocking {
        if let Err(e) = settings::credentials::clear_all_cooldowns(pool).await {
            tracing::warn!(
                module = "captain",
                task_id = id,
                error = %e,
                "failed to clear credential cooldowns"
            );
        }
    }
    Ok(())
}

#[tracing::instrument(skip_all)]
pub async fn retry_item(store: &TaskStore, id: i64) -> Result<()> {
    let mut task = store
        .find_by_id(id)
        .await?
        .ok_or(crate::TaskActionError::NotFound(id))?;
    anyhow::ensure!(
        task.status == ItemStatus::Errored,
        "retry requires status errored, got {:?}",
        task.status
    );
    let trigger = task
        .captain_review_trigger
        .unwrap_or(crate::ReviewTrigger::Retry);
    super::super::action_contract::reset_review_retry(&mut task, trigger);
    task.last_activity_at = Some(global_types::now_rfc3339());
    let event = TimelineEvent {
        timestamp: global_types::now_rfc3339(),
        actor: "human".into(),
        summary: "Retried — re-entering captain review".into(),
        data: TimelineEventPayload::StatusChanged {
            from: api_types::ItemStatus::Errored,
            to: api_types::ItemStatus::CaptainReviewing,
        },
    };
    let bus_payload = crate::io::queries::tasks_persist::task_bus_effect(id, "updated");
    let touch_payload =
        crate::io::queries::tasks_persist::workbench_touch_effect(task.workbench_id);
    let applied = crate::io::queries::tasks::persist_status_transition_with_command_and_effects(
        store.pool(),
        &task,
        ItemStatus::Errored.as_str(),
        TaskLifecycleCommand::RetryReview.as_str(),
        &event,
        vec![
            global_db::lifecycle::LifecycleEffect {
                effect_kind: "task.bus.publish",
                payload: &bus_payload,
            },
            global_db::lifecycle::LifecycleEffect {
                effect_kind: "task.workbench.touch",
                payload: &touch_payload,
            },
        ],
    )
    .await?;
    if !applied {
        return Err(crate::TaskActionError::Conflict {
            message: format!("task {id} changed concurrently while retrying review"),
        }
        .into());
    }
    Ok(())
}

#[tracing::instrument(skip_all)]
pub async fn stop_all_workers(store: &TaskStore, pool: &sqlx::SqlitePool) -> Result<u32> {
    let mut killed = 0u32;
    for item in store.load_all().await? {
        if item.status != ItemStatus::InProgress {
            continue;
        }
        let cc_sid = item.session_ids.worker.as_deref().unwrap_or("");
        if !cc_sid.is_empty() {
            let was_alive = crate::io::pid_registry::get_pid(cc_sid)
                .is_some_and(|pid| pid.as_u32() > 0 && global_claude::is_process_alive(pid));
            crate::io::session_terminate::terminate_session(
                pool,
                cc_sid,
                global_types::SessionStatus::Stopped,
                None,
            )
            .await;
            if was_alive {
                killed += 1;
            }
        }
    }
    Ok(killed)
}

#[tracing::instrument(skip_all)]
pub async fn merge_pr(store: &TaskStore, pr_number: i64, project: &str) -> Result<MergeResponse> {
    let items = store.load_all().await?;
    let item = items
        .iter()
        .find(|it| {
            it.pr_number == Some(pr_number)
                && (project.is_empty() || it.project.eq_ignore_ascii_case(project))
        })
        .ok_or_else(|| anyhow::anyhow!("no task found for PR #{pr_number} in {project}"))?;

    anyhow::ensure!(
        item.status == ItemStatus::AwaitingReview || item.status == ItemStatus::HandedOff,
        "item must be in awaiting-review or handed-off to merge, got {:?}",
        item.status
    );

    let mut task = item.clone();
    let _ = apply_manual_command(&mut task, TaskLifecycleCommand::StartMerge)?;
    task.last_activity_at = Some(global_types::now_rfc3339());
    task.session_ids.merge = None;
    task.merge_fail_count = 0;

    let event = TimelineEvent {
        timestamp: global_types::now_rfc3339(),
        actor: "human".into(),
        summary: format!("Queued captain merge for PR #{pr_number}"),
        data: TimelineEventPayload::StatusChangedRetryMerge {
            from: item.status.into(),
            to: api_types::ItemStatus::CaptainMerging,
            pr: pr_number,
        },
    };
    let bus_payload = crate::io::queries::tasks_persist::task_bus_effect(task.id, "updated");
    let touch_payload =
        crate::io::queries::tasks_persist::workbench_touch_effect(task.workbench_id);
    let applied = crate::io::queries::tasks::persist_status_transition_with_command_and_effects(
        store.pool(),
        &task,
        item.status.as_str(),
        TaskLifecycleCommand::StartMerge.as_str(),
        &event,
        vec![
            global_db::lifecycle::LifecycleEffect {
                effect_kind: "task.bus.publish",
                payload: &bus_payload,
            },
            global_db::lifecycle::LifecycleEffect {
                effect_kind: "task.workbench.touch",
                payload: &touch_payload,
            },
        ],
    )
    .await?;
    if !applied {
        return Err(crate::TaskActionError::Conflict {
            message: format!("task {} changed concurrently while queueing merge", task.id),
        }
        .into());
    }

    tracing::info!(
        module = "captain",
        item_id = task.id,
        pr = pr_number,
        "item transitioned to CaptainMerging — merge session will be spawned on next tick"
    );

    Ok(MergeResponse {
        status: api_types::ItemStatus::CaptainMerging,
        item_id: task.id,
        pr: pr_number,
    })
}

#[tracing::instrument(skip_all)]
pub async fn bulk_update_tasks(
    store: &TaskStore,
    ids: &[i64],
    updates: UpdateTaskInput,
    _pool: &sqlx::SqlitePool,
) -> Result<()> {
    for id in ids {
        update_task(store, *id, updates.clone()).await?;
    }
    Ok(())
}
