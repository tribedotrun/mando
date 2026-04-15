//! Dashboard API — functions called by the gateway for REST endpoints.

use anyhow::{Context, Result};
use mando_config::settings::Config;
use mando_types::task::{ItemStatus, Task};

use crate::io::task_store::TaskStore;
use crate::runtime::task_notes::append_tagged_note;

pub(crate) fn truncate_utf8(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    &s[..s.floor_char_boundary(max)]
}

/// Bail if the task is in a captain-managed transient state that forbids
/// manual intervention (review or merge in progress).
fn ensure_not_captain_busy(task: &Task, action: &str) -> Result<()> {
    match task.status {
        ItemStatus::CaptainReviewing => {
            anyhow::bail!(
                "cannot {action} item {}: captain review is in progress",
                task.id
            )
        }
        ItemStatus::CaptainMerging => {
            anyhow::bail!(
                "cannot {action} item {}: captain merge is in progress",
                task.id
            )
        }
        _ => Ok(()),
    }
}

pub async fn add_task(
    config: &Config,
    store: &TaskStore,
    title: &str,
    project: Option<&str>,
    source: Option<&str>,
) -> Result<serde_json::Value> {
    let projects = &config.captain.projects;
    let (resolved_project, clean_title) = if let Some(r) = project {
        let name =
            mando_config::resolve_project_config(Some(r), config).map(|(_, pc)| pc.name.clone());
        if name.is_none() {
            let mut valid: Vec<&str> = projects.values().map(|pc| pc.name.as_str()).collect();
            valid.sort_unstable();
            anyhow::bail!(
                "unknown project {r:?} — valid projects: {}",
                if valid.is_empty() {
                    "(none configured)".to_string()
                } else {
                    valid.join(", ")
                }
            );
        }
        (name, title.to_string())
    } else {
        let (matched, cleaned) = mando_config::match_project_by_prefix(title, projects);
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
        let resolved = mando_config::resolve_project_config(Some(name), config);
        let (path, github_repo) = match resolved {
            Some((_, pc)) => (pc.path.as_str(), pc.github_repo.as_deref()),
            None => ("", None),
        };
        let id = mando_db::queries::projects::upsert(store.pool(), name, path, github_repo).await?;
        (id, name.clone())
    } else {
        if projects.is_empty() {
            anyhow::bail!("no project configured — add a project before creating tasks");
        }
        anyhow::bail!("project selection required — choose a project before creating tasks");
    };

    let mut new_task = mando_types::Task::new(&clean_title);
    new_task.status = ItemStatus::New;
    new_task.project_id = project_id;
    new_task.project = project_name;
    new_task.original_prompt = Some(title.to_string());
    new_task.created_at = Some(mando_types::now_rfc3339());
    new_task.source = source.map(String::from);

    let id = store.add(new_task).await?;

    // Emit Created timeline event at creation time.
    let _ = super::timeline_emit::emit(
        store.pool(),
        id,
        mando_types::timeline::TimelineEventType::Created,
        "captain",
        &format!("Item created: {clean_title}"),
        serde_json::json!({ "source": source }),
    )
    .await;

    Ok(serde_json::json!({
        "id": id,
        "title": title,
    }))
}

pub async fn update_task(store: &TaskStore, id: i64, updates: &serde_json::Value) -> Result<()> {
    store.update_fields(id, updates).await
}

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
pub async fn trigger_captain_tick(
    config: &Config,
    workflow: &mando_config::workflow::CaptainWorkflow,
    dry_run: bool,
    bus: Option<&mando_shared::EventBus>,
    emit_notifications: bool,
    store_lock: &std::sync::Arc<tokio::sync::RwLock<TaskStore>>,
    cancel: &tokio_util::sync::CancellationToken,
) -> Result<serde_json::Value> {
    let result = super::tick::run_captain_tick(
        config,
        workflow,
        dry_run,
        bus,
        emit_notifications,
        store_lock,
        cancel,
    )
    .await?;
    Ok(crate::biz::tick_summary::tick_result_to_json(&result))
}

pub async fn add_task_with_context(
    config: &Config,
    store: &TaskStore,
    title: &str,
    project: Option<&str>,
    context: Option<&str>,
    source: Option<&str>,
) -> Result<serde_json::Value> {
    let result = add_task(config, store, title, project, source).await?;
    if let Some(ctx) = context {
        if let Some(id) = result["id"].as_i64() {
            update_task(store, id, &serde_json::json!({"context": ctx})).await?;
        }
    }
    Ok(result)
}

pub async fn accept_item(store: &TaskStore, id: i64) -> Result<()> {
    update_task(store, id, &serde_json::json!({"status": "merged"})).await
}

pub async fn cancel_item(store: &TaskStore, id: i64, pool: &sqlx::SqlitePool) -> Result<()> {
    // Terminate ALL running sessions for this task. If we can't enumerate
    // them we must NOT mark the task canceled; otherwise live processes
    // would be orphaned while the UI shows them as stopped.
    let running = mando_db::queries::sessions::list_running_sessions_for_task(pool, id)
        .await
        .with_context(|| format!("cancel_item: failed to list running sessions for task {id}"))?;
    for row in &running {
        crate::io::session_terminate::terminate_session(
            pool,
            &row.session_id,
            mando_types::SessionStatus::Stopped,
            None,
        )
        .await;
    }
    update_task(store, id, &serde_json::json!({"status": "canceled"})).await
}

pub async fn force_update_task(
    store: &TaskStore,
    id: i64,
    updates: &serde_json::Value,
) -> Result<()> {
    store.force_update_fields(id, updates).await
}

pub async fn rework_item(store: &TaskStore, id: i64, feedback: &str) -> Result<()> {
    let id_str = id.to_string();
    let _lock = crate::io::item_lock::acquire_item_lock(&id_str, "rework")?;

    if let Some(new_context) = {
        let task = store
            .find_by_id(id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("task not found: {}", id))?;
        ensure_not_captain_busy(&task, "rework")?;
        append_tagged_note(task.context.as_deref(), "Rework feedback", feedback)
    } {
        store
            .update(id, |t| {
                t.context = Some(new_context.clone());
            })
            .await?;
    }

    update_task(store, id, &serde_json::json!({"status": "rework"})).await
}

pub async fn handoff_item(store: &TaskStore, id: i64, pool: &sqlx::SqlitePool) -> Result<()> {
    let id_str = id.to_string();
    let _lock = crate::io::item_lock::acquire_item_lock(&id_str, "handoff")?;

    if let Some(full) = store.find_by_id(id).await? {
        let cc_sid = full.session_ids.worker.as_deref().unwrap_or("");
        if !cc_sid.is_empty() {
            crate::io::session_terminate::terminate_session(
                pool,
                cc_sid,
                mando_types::SessionStatus::Stopped,
                None,
            )
            .await;
        }
    }
    update_task(store, id, &serde_json::json!({"status": "handed-off"})).await
}

/// Validate that a task is blocked by a rate-limit cooldown (ambient or
/// per-credential), then clear all active cooldowns so the next captain tick
/// can resume it.
pub async fn validate_rate_limited_task(store: &TaskStore, id: i64) -> Result<()> {
    let task = store
        .find_by_id(id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("task not found: {}", id))?;
    anyhow::ensure!(
        matches!(
            task.status,
            ItemStatus::CaptainReviewing | ItemStatus::CaptainMerging | ItemStatus::Clarifying
        ),
        "resume requires captain-reviewing/captain-merging/clarifying, got {:?}",
        task.status
    );

    let pool = store.pool();
    let ambient_active = super::ambient_rate_limit::is_active();
    let credential_blocking =
        mando_db::queries::credentials::earliest_cooldown_remaining_secs(pool).await > 0;
    anyhow::ensure!(
        ambient_active || credential_blocking,
        "rate-limit cooldown is not active"
    );
    if ambient_active {
        super::ambient_rate_limit::clear();
    }
    if credential_blocking {
        if let Err(e) = mando_db::queries::credentials::clear_all_cooldowns(pool).await {
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

pub async fn retry_item(store: &TaskStore, id: i64) -> Result<()> {
    let mut task = store
        .find_by_id(id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("task not found: {}", id))?;
    anyhow::ensure!(
        task.status == ItemStatus::Errored,
        "retry requires status errored, got {:?}",
        task.status
    );
    let trigger = task
        .captain_review_trigger
        .unwrap_or(mando_types::task::ReviewTrigger::Retry);
    super::action_contract::reset_review_retry(&mut task, trigger);
    store.write_task(&task).await?;
    Ok(())
}

pub async fn stop_all_workers(store: &TaskStore, pool: &sqlx::SqlitePool) -> Result<u32> {
    let mut killed = 0u32;
    for item in store.load_all().await? {
        if item.status != ItemStatus::InProgress {
            continue;
        }
        let cc_sid = item.session_ids.worker.as_deref().unwrap_or("");
        if !cc_sid.is_empty() {
            // Only count as killed if the process was actually alive.
            let was_alive = crate::io::pid_registry::get_pid(cc_sid)
                .is_some_and(|pid| pid.as_u32() > 0 && mando_cc::is_process_alive(pid));
            crate::io::session_terminate::terminate_session(
                pool,
                cc_sid,
                mando_types::SessionStatus::Stopped,
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

/// Transition an item to CaptainMerging. The captain tick will spawn an AI
/// session to check CI, trigger it if needed, and merge when green.
pub async fn merge_pr(
    store: &TaskStore,
    pr_number: i64,
    project: &str,
) -> Result<serde_json::Value> {
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

    let id = item.id;
    store
        .force_update_fields(
            id,
            &serde_json::json!({
                "status": "captain-merging",
                "session_ids": {
                    "worker": item.session_ids.worker,
                    "review": item.session_ids.review,
                    "clarifier": item.session_ids.clarifier,
                    "merge": null,
                },
                "merge_fail_count": 0,
            }),
        )
        .await?;

    tracing::info!(
        module = "captain",
        item_id = id,
        pr = pr_number,
        "item transitioned to CaptainMerging — merge session will be spawned on next tick"
    );

    Ok(serde_json::json!({
        "status": "captain-merging",
        "item_id": id,
        "pr": pr_number,
    }))
}

pub async fn bulk_update_tasks(
    store: &TaskStore,
    ids: &[i64],
    updates: serde_json::Value,
    pool: &sqlx::SqlitePool,
) -> Result<()> {
    let is_cancel = updates.get("status").and_then(|v| v.as_str()) == Some("canceled");

    for id in ids {
        if is_cancel {
            cancel_item(store, *id, pool).await?;
        } else {
            update_task(store, *id, &updates).await?;
        }
    }
    Ok(())
}
