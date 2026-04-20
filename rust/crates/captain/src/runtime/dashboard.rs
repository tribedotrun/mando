//! Dashboard API — functions called by the gateway for REST endpoints.

use api_types::TaskCreateResponse;

use crate::{ItemStatus, Task, TimelineEvent, TimelineEventPayload, UpdateTaskInput};
use anyhow::{Context, Result};
use settings::config::settings::Config;

use crate::io::task_store::TaskStore;
use crate::runtime::task_notes::append_tagged_note;
use crate::service::lifecycle::{apply_manual_command, TaskLifecycleCommand};

mod actions;

pub(crate) use crate::service::text::truncate_utf8;
pub use actions::{
    bulk_update_tasks, merge_pr, retry_item, stop_all_workers, validate_rate_limited_task,
};

/// Bail if the task is in a captain-managed transient state that forbids
/// manual intervention (review or merge in progress).
fn ensure_not_captain_busy(task: &Task, action: &str) -> Result<()> {
    let reason = match task.status {
        ItemStatus::CaptainReviewing => "captain review is in progress",
        ItemStatus::CaptainMerging => "captain merge is in progress",
        _ => return Ok(()),
    };
    Err(crate::TaskActionError::Conflict {
        message: format!("cannot {action} item {}: {reason}", task.id),
    }
    .into())
}

async fn apply_task_lifecycle_command(
    store: &TaskStore,
    id: i64,
    command: TaskLifecycleCommand,
    summary: String,
    data: TimelineEventPayload,
) -> Result<()> {
    let mut task = store
        .find_by_id(id)
        .await?
        .ok_or(crate::TaskActionError::NotFound(id))?;
    let from_status = task.status;
    let _ = apply_manual_command(&mut task, command)?;
    if command == TaskLifecycleCommand::Queue && from_status == ItemStatus::PlanReady {
        task.planning = false;
    }
    task.last_activity_at = Some(global_types::now_rfc3339());
    let event = TimelineEvent {
        timestamp: global_types::now_rfc3339(),
        actor: "human".into(),
        summary,
        data,
    };
    let bus_payload = crate::io::queries::tasks_persist::task_bus_effect(id, "updated");
    let touch_payload =
        crate::io::queries::tasks_persist::workbench_touch_effect(task.workbench_id);
    let applied = crate::io::queries::tasks::persist_status_transition_with_command_and_effects(
        store.pool(),
        &task,
        from_status.as_str(),
        command.as_str(),
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
            message: format!("task {id} changed concurrently while applying lifecycle command"),
        }
        .into());
    }
    Ok(())
}

#[tracing::instrument(skip_all)]
pub async fn add_task(
    config: &Config,
    store: &TaskStore,
    title: &str,
    project: Option<&str>,
    source: Option<&str>,
) -> Result<TaskCreateResponse> {
    let projects = &config.captain.projects;
    let (resolved_project, clean_title) = if let Some(r) = project {
        let name = settings::config::resolve_project_config(Some(r), config)
            .map(|(_, pc)| pc.name.clone());
        if name.is_none() {
            let mut valid: Vec<String> = projects.values().map(|pc| pc.name.clone()).collect();
            valid.sort();
            return Err(crate::TaskCreateError::UnknownProject {
                name: r.to_string(),
                valid,
            }
            .into());
        }
        (name, title.to_string())
    } else {
        let (matched, cleaned) = settings::config::match_project_by_prefix(title, projects);
        if matched.is_some() {
            (matched, cleaned.to_string())
        } else if let Some(only) = projects.values().next().filter(|_| projects.len() == 1) {
            (Some(only.name.clone()), title.to_string())
        } else {
            (None, title.to_string())
        }
    };

    // Resolve project_id, upserting into the projects table if needed.
    let (project_id, project_name) = if let Some(ref name) = resolved_project {
        let resolved = settings::config::resolve_project_config(Some(name), config);
        let (path, github_repo) = match resolved {
            Some((_, pc)) => (pc.path.as_str(), pc.github_repo.as_deref()),
            None => ("", None),
        };
        let id = settings::projects::upsert(store.pool(), name, path, github_repo).await?;
        (id, name.clone())
    } else if projects.is_empty() {
        return Err(crate::TaskCreateError::NoProjectConfigured.into());
    } else {
        return Err(crate::TaskCreateError::ProjectSelectionRequired.into());
    };

    let mut new_task = crate::Task::new(&clean_title);
    new_task.project_id = project_id;
    new_task.project = project_name;
    new_task.original_prompt = Some(title.to_string());
    new_task.created_at = Some(global_types::now_rfc3339());
    new_task.source = source.map(String::from);

    let id = store.add(new_task).await?;

    Ok(TaskCreateResponse {
        id,
        title: title.to_string(),
    })
}

#[tracing::instrument(skip_all)]
pub async fn update_task(store: &TaskStore, id: i64, updates: UpdateTaskInput) -> Result<()> {
    store.update_fields(id, updates).await
}

#[tracing::instrument(skip_all)]
pub async fn delete_tasks(
    config: &Config,
    store: &TaskStore,
    ids: &[i64],
    opts: &crate::io::task_cleanup::CleanupOptions,
) -> Result<Vec<String>> {
    let mut to_delete = Vec::new();
    for id in ids {
        if let Some(task) = store.find_by_id(*id).await? {
            to_delete.push(task);
        }
    }

    if !opts.force {
        for task in &to_delete {
            if task.status.is_active() {
                anyhow::bail!(
                    "task {} ({}) has an active worker (status: {}); \
                     use `mando captain stop {}` to stop it first, \
                     or force-delete from the Electron UI",
                    task.id,
                    task.title,
                    task.status,
                    task.id,
                );
            }
        }
    }

    let warnings =
        crate::io::task_cleanup::cleanup_tasks(&to_delete, config, store.pool(), opts).await;
    for id in ids {
        store.remove(*id).await?;
    }
    Ok(warnings)
}

#[tracing::instrument(skip_all, fields(module = "captain"))]
#[allow(clippy::too_many_arguments)]
pub async fn trigger_captain_tick(
    config: &Config,
    workflow: &settings::config::workflow::CaptainWorkflow,
    dry_run: bool,
    bus: Option<&global_bus::EventBus>,
    emit_notifications: bool,
    store_lock: &std::sync::Arc<tokio::sync::RwLock<TaskStore>>,
    cancel: &tokio_util::sync::CancellationToken,
    task_tracker: &tokio_util::task::TaskTracker,
) -> Result<crate::TickResult> {
    super::tick::run_captain_tick(
        config,
        workflow,
        dry_run,
        bus,
        emit_notifications,
        store_lock,
        cancel,
        task_tracker,
    )
    .await
}

#[tracing::instrument(skip_all)]
pub async fn add_task_with_context(
    config: &Config,
    store: &TaskStore,
    title: &str,
    project: Option<&str>,
    context: Option<&str>,
    source: Option<&str>,
) -> Result<TaskCreateResponse> {
    let result = add_task(config, store, title, project, source).await?;
    if let Some(ctx) = context {
        update_task(
            store,
            result.id,
            UpdateTaskInput {
                context: Some(Some(ctx.to_string())),
                ..Default::default()
            },
        )
        .await?;
    }
    Ok(result)
}

#[tracing::instrument(skip_all)]
pub async fn queue_item(store: &TaskStore, id: i64, reason: &str) -> Result<()> {
    apply_task_lifecycle_command(
        store,
        id,
        TaskLifecycleCommand::Queue,
        "Queued for captain dispatch".to_string(),
        TimelineEventPayload::StatusChangedQueued {
            to: "queued".to_string(),
            reason: reason.to_string(),
        },
    )
    .await
}

#[tracing::instrument(skip_all)]
pub async fn accept_item(store: &TaskStore, id: i64) -> Result<()> {
    apply_task_lifecycle_command(
        store,
        id,
        TaskLifecycleCommand::Accept,
        "Accepted by human".to_string(),
        TimelineEventPayload::AcceptedNoPr {
            accepted_by: "human".to_string(),
        },
    )
    .await
}

#[tracing::instrument(skip_all)]
pub async fn cancel_item(store: &TaskStore, id: i64, pool: &sqlx::SqlitePool) -> Result<()> {
    // Terminate ALL running sessions for this task. If we can't enumerate
    // them we must NOT mark the task canceled; otherwise live processes
    // would be orphaned while the UI shows them as stopped.
    let running = sessions_db::list_running_sessions_for_task(pool, id)
        .await
        .with_context(|| format!("cancel_item: failed to list running sessions for task {id}"))?;
    for row in &running {
        crate::io::session_terminate::terminate_session(
            pool,
            &row.session_id,
            global_types::SessionStatus::Stopped,
            None,
        )
        .await;
    }
    apply_task_lifecycle_command(
        store,
        id,
        TaskLifecycleCommand::Cancel,
        "Canceled by human".to_string(),
        TimelineEventPayload::CanceledByHuman {
            canceled_by: "human".to_string(),
        },
    )
    .await
}

#[tracing::instrument(skip_all)]
pub async fn set_task_planning(store: &TaskStore, id: i64, planning: bool) -> Result<()> {
    store.set_planning(id, planning).await
}

#[tracing::instrument(skip_all)]
pub async fn rework_item(store: &TaskStore, id: i64, feedback: &str) -> Result<()> {
    let id_str = id.to_string();
    let _lock = crate::io::item_lock::acquire_item_lock(&id_str, "rework")?;

    let task = store
        .find_by_id(id)
        .await?
        .ok_or(crate::TaskActionError::NotFound(id))?;
    ensure_not_captain_busy(&task, "rework")?;
    if let Some(new_context) =
        append_tagged_note(task.context.as_deref(), "Rework feedback", feedback)
    {
        store
            .update(id, |t| {
                t.context = Some(new_context.clone());
            })
            .await?;
    }

    let summary = if feedback.is_empty() {
        "Rework requested".to_string()
    } else {
        format!("Rework requested: {feedback}")
    };
    apply_task_lifecycle_command(
        store,
        id,
        TaskLifecycleCommand::Rework,
        summary,
        TimelineEventPayload::ReworkRequested {
            content: feedback.to_string(),
            to: "rework".to_string(),
        },
    )
    .await
}

#[tracing::instrument(skip_all)]
pub async fn handoff_item(store: &TaskStore, id: i64, pool: &sqlx::SqlitePool) -> Result<()> {
    let id_str = id.to_string();
    let _lock = crate::io::item_lock::acquire_item_lock(&id_str, "handoff")?;

    if let Some(full) = store.find_by_id(id).await? {
        let cc_sid = full.session_ids.worker.as_deref().unwrap_or("");
        if !cc_sid.is_empty() {
            crate::io::session_terminate::terminate_session(
                pool,
                cc_sid,
                global_types::SessionStatus::Stopped,
                None,
            )
            .await;
        }
    }
    apply_task_lifecycle_command(
        store,
        id,
        TaskLifecycleCommand::Handoff,
        "Handed off by human".to_string(),
        TimelineEventPayload::HandedOff {
            to: "handed-off".to_string(),
            handed_off_by: "human".to_string(),
        },
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn bulk_update_tasks_updates_worker_editorial_field() {
        let db = global_db::Db::open_in_memory().await.unwrap();
        let store = TaskStore::new(db.pool().clone());
        let project_id = settings::projects::upsert(db.pool(), "test", "", None)
            .await
            .unwrap();

        let mut task_a = crate::Task::new("a");
        task_a.project_id = project_id;
        task_a.project = "test".into();
        let mut task_b = crate::Task::new("b");
        task_b.project_id = project_id;
        task_b.project = "test".into();

        let id_a = store.add(task_a).await.unwrap();
        let id_b = store.add(task_b).await.unwrap();

        bulk_update_tasks(
            &store,
            &[id_a, id_b],
            UpdateTaskInput {
                worker: Some(Some("worker-1".into())),
                ..Default::default()
            },
            db.pool(),
        )
        .await
        .unwrap();

        let task_a = store.find_by_id(id_a).await.unwrap().unwrap();
        let task_b = store.find_by_id(id_b).await.unwrap().unwrap();
        assert_eq!(task_a.worker.as_deref(), Some("worker-1"));
        assert_eq!(task_b.worker.as_deref(), Some("worker-1"));
    }
}
