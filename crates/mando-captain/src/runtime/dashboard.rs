//! Dashboard API — functions called by the gateway for REST endpoints.

use anyhow::Result;
use mando_config::settings::Config;
use mando_config::workflow::CaptainWorkflow;
use mando_types::task::ItemStatus;

use crate::io::task_store::TaskStore;
use crate::runtime::task_notes::append_tagged_note;

pub(crate) fn truncate_utf8(s: &str, max: usize) -> &str {
    if s.len() <= max {
        return s;
    }
    &s[..s.floor_char_boundary(max)]
}

pub async fn add_task(
    config: &Config,
    store: &TaskStore,
    title: &str,
    project: Option<&str>,
) -> Result<serde_json::Value> {
    let projects = &config.captain.projects;
    let (resolved_project, clean_title) = if let Some(r) = project {
        let name = mando_config::resolve_project_config(Some(r), config)
            .map(|(_, pc)| pc.name.clone())
            .unwrap_or_else(|| r.to_string());
        (Some(name), title.to_string())
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

    let github_repo = mando_config::resolve_github_repo(resolved_project.as_deref(), config);

    let mut new_task = mando_types::Task::new(&clean_title);
    new_task.status = ItemStatus::New;
    new_task.project = resolved_project;
    new_task.github_repo = github_repo;
    new_task.original_prompt = Some(title.to_string());
    new_task.created_at = Some(mando_types::now_rfc3339());

    let id = store.add(new_task).await?;

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
) -> Result<serde_json::Value> {
    let result = super::tick::run_captain_tick(
        config,
        workflow,
        dry_run,
        bus,
        emit_notifications,
        store_lock,
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
) -> Result<serde_json::Value> {
    let result = add_task(config, store, title, project).await?;
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
    // Terminate ALL running sessions for this task.
    let numeric_id = id.to_string();

    for tid in &[numeric_id.as_str()] {
        match mando_db::queries::sessions::list_running_sessions_for_task(pool, tid).await {
            Ok(running) => {
                for row in &running {
                    crate::io::session_terminate::terminate_session(
                        pool,
                        &row.session_id,
                        mando_types::SessionStatus::Stopped,
                        None,
                    )
                    .await;
                }
            }
            Err(e) => {
                tracing::error!(
                    module = "captain",
                    task_id = %tid,
                    error = %e,
                    "failed to query running sessions for cancel — processes may still be running"
                );
            }
        }
    }
    update_task(store, id, &serde_json::json!({"status": "canceled"})).await
}

pub async fn reopen_item(store: &TaskStore, id: i64, feedback: &str) -> Result<()> {
    let task = store
        .find_by_id(id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("task not found: {}", id))?;

    anyhow::ensure!(
        task.status != ItemStatus::CaptainReviewing && task.status != ItemStatus::CaptainMerging,
        "cannot reopen item {}: captain {} is in progress",
        id,
        if task.status == ItemStatus::CaptainReviewing {
            "review"
        } else {
            "merge"
        }
    );

    if let Some(new_context) =
        append_tagged_note(task.context.as_deref(), "Reopen feedback", feedback)
    {
        store
            .update(id, |t| {
                t.context = Some(new_context.clone());
            })
            .await?;
    }

    let task = store
        .find_by_id(id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("task not found: {}", id))?;
    let can_resume =
        task.worker.is_some() && task.session_ids.worker.is_some() && task.worktree.is_some();
    let status = if can_resume { "in-progress" } else { "queued" };
    let new_seq = task.reopen_seq + 1;
    let new_intervention_count = task.intervention_count + 1;
    let updates = serde_json::json!({
        "status": status,
        "intervention_count": new_intervention_count,
        "reopen_source": "human",
        "reopen_seq": new_seq,
    });
    force_update_task(store, id, &updates).await
}

pub async fn resume_reopened_worker(
    item: &mando_types::Task,
    feedback: &str,
    workflow: &CaptainWorkflow,
    pool: &sqlx::SqlitePool,
) -> Result<()> {
    let worker = item.worker.as_deref().unwrap_or("");
    let cc_sid = item.session_ids.worker.as_deref().unwrap_or("");
    let wt = item.worktree.as_deref().unwrap_or("");

    anyhow::ensure!(
        !worker.is_empty() && !cc_sid.is_empty() && !wt.is_empty(),
        "item missing worker/session/worktree — cannot resume"
    );

    let old_pid = crate::io::pid_registry::get_pid(cc_sid).unwrap_or(0);
    if old_pid > 0 {
        if let Err(e) = mando_cc::kill_process(old_pid).await {
            tracing::warn!(
                module = "captain",
                worker = worker,
                pid = old_pid,
                error = %e,
                "failed to kill stale worker before resume"
            );
        }
    }

    let wt_path = mando_config::expand_tilde(wt);
    anyhow::ensure!(
        wt_path.exists() && wt_path.is_dir(),
        "worktree path does not exist: {}",
        wt_path.display()
    );
    let reopen_seq = item.reopen_seq;

    let ai_dir = wt_path.join(".ai");
    std::fs::create_dir_all(&ai_dir)?;
    std::fs::write(
        ai_dir.join("captain-reopen-context.md"),
        format!(
            "# Captain Reopen (seq={})\n\nHuman feedback:\n{}\n\nAddress the feedback, then post an ack comment: `[Mando] Reopen #{} addressed: <summary>`\n",
            reopen_seq, feedback, reopen_seq
        ),
    )?;

    let seq_str = reopen_seq.to_string();
    let mut vars = std::collections::HashMap::new();
    vars.insert("reopen_seq", seq_str.as_str());
    let msg = mando_config::render_prompt("reopen_resume", &workflow.prompts, &vars)
        .map_err(|e| anyhow::anyhow!(e))?;

    let stream_path = mando_config::stream_path_for_session(cc_sid);
    if mando_cc::stream_has_broken_session(&stream_path) {
        anyhow::bail!(
            "no init event in stream for {} — session was never created, cannot resume",
            cc_sid
        );
    }

    let stream_size_before = mando_cc::get_stream_file_size(&stream_path);

    let (pid, _) = crate::io::process_manager::resume_worker_process(
        worker,
        &msg,
        &wt_path,
        "default",
        cc_sid,
        &std::collections::HashMap::new(),
        workflow.models.fallback.as_deref(),
    )
    .await?;

    // Register PID in the session registry.
    crate::io::pid_registry::register(cc_sid, pid);

    let health_path = mando_config::worker_health_path();
    let mut hstate = crate::io::health_store::load_health_state(&health_path);
    crate::io::health_store::set_health_field(
        &mut hstate,
        worker,
        "stream_size_at_spawn",
        serde_json::json!(stream_size_before),
    );
    if let Err(e) = crate::io::health_store::save_health_state(&health_path, &hstate) {
        tracing::error!(module = "captain", worker = %worker, error = %e, "failed to persist health state");
    }
    crate::io::headless_cc::log_running_session(
        pool,
        cc_sid,
        &wt_path,
        "worker",
        worker,
        &item.id.to_string(),
        true,
    )
    .await;

    tracing::info!(
        module = "captain",
        worker = worker,
        pid = pid,
        reopen_seq = reopen_seq,
        "reopened worker via resume"
    );
    Ok(())
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
        anyhow::ensure!(
            task.status != ItemStatus::CaptainReviewing
                && task.status != ItemStatus::CaptainMerging,
            "cannot rework item {}: captain {} is in progress",
            id,
            if task.status == ItemStatus::CaptainReviewing {
                "review"
            } else {
                "merge"
            }
        );
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
                .is_some_and(|pid| pid > 0 && mando_cc::is_process_alive(pid));
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
pub async fn merge_pr(store: &TaskStore, pr_number: &str, repo: &str) -> Result<serde_json::Value> {
    // Normalize any input format (#N, bare N, full URL) to a bare number string.
    let num = mando_types::task::extract_pr_number(pr_number)
        .ok_or_else(|| anyhow::anyhow!("invalid PR reference: {pr_number}"))?;
    let url_suffix = format!("/pull/{num}");
    let items = store.load_all().await?;
    let item = items
        .iter()
        .find(|it| {
            it.pr
                .as_deref()
                .is_some_and(|pr| pr == num || (pr.contains(repo) && pr.ends_with(&url_suffix)))
        })
        .ok_or_else(|| anyhow::anyhow!("no task found for PR #{num} in {repo}"))?;

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
