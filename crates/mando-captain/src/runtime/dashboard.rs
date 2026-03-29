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

    let mut new_task = mando_types::Task::new(&clean_title);
    new_task.status = ItemStatus::New;
    new_task.project = resolved_project;
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

pub async fn delete_tasks(config: &Config, store: &TaskStore, ids: &[i64]) -> Result<()> {
    let mut to_delete = Vec::new();
    for id in ids {
        if let Some(task) = store.find_by_id(*id).await? {
            to_delete.push(task);
        }
    }
    crate::io::task_cleanup::cleanup_tasks(&to_delete, config, store.pool()).await;
    for id in ids {
        store.remove(*id).await?;
    }
    Ok(())
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

pub async fn cancel_item(store: &TaskStore, id: i64) -> Result<()> {
    update_task(store, id, &serde_json::json!({"status": "canceled"})).await
}

pub async fn reopen_item(store: &TaskStore, id: i64, feedback: &str) -> Result<()> {
    let task = store
        .find_by_id(id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("task not found: {}", id))?;

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

    let old_pid = crate::io::health_store::get_pid_for_worker(worker);
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

    let health_path = mando_config::worker_health_path();
    let mut hstate = crate::io::health_store::load_health_state(&health_path);
    crate::io::health_store::set_health_field(&mut hstate, worker, "pid", serde_json::json!(pid));
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
        &item.best_id(),
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

pub async fn handoff_item(store: &TaskStore, id: i64) -> Result<()> {
    let id_str = id.to_string();
    let _lock = crate::io::item_lock::acquire_item_lock(&id_str, "handoff")?;

    if let Some(full) = store.find_by_id(id).await? {
        if let Some(ref worker) = full.worker {
            let health_path = mando_config::worker_health_path();
            let health_state = crate::io::health_store::load_health_state(&health_path);
            let pid = crate::io::health_store::get_health_u32(&health_state, worker, "pid");
            if pid > 0 && mando_cc::is_process_alive(pid) {
                if let Err(e) = mando_cc::kill_process(pid).await {
                    tracing::warn!(module = "captain", worker = %worker, pid = pid, error = %e, "failed to kill worker");
                }
            }
        }
    }
    update_task(store, id, &serde_json::json!({"status": "handed-off"})).await
}

pub async fn retry_item(store: &TaskStore, id: i64) -> Result<()> {
    let task = store
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
    force_update_task(
        store,
        id,
        &serde_json::json!({
            "status": "captain-reviewing",
            "captain_review_trigger": trigger,
        }),
    )
    .await
}

pub async fn answer_clarification(store: &TaskStore, id: i64, answer: &str) -> Result<()> {
    let task = store
        .find_by_id(id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("task not found: {}", id))?;
    anyhow::ensure!(
        task.status == ItemStatus::NeedsClarification,
        "answer requires status needs-clarification, got {:?}",
        task.status
    );
    let answer = answer.trim();
    anyhow::ensure!(!answer.is_empty(), "answer text must not be empty");
    let new_context = append_tagged_note(task.context.as_deref(), "Human answer", answer)
        .expect("validated answer should always produce a note");
    store
        .update(id, |t| {
            t.context = Some(new_context.clone());
        })
        .await?;
    force_update_task(store, id, &serde_json::json!({"status": "clarifying"})).await
}

pub async fn stop_all_workers(store: &TaskStore) -> Result<u32> {
    let health_path = mando_config::worker_health_path();
    let health_state = crate::io::health_store::load_health_state(&health_path);

    let mut killed = 0u32;
    for rt in store.routing().await? {
        if rt.status != ItemStatus::InProgress {
            continue;
        }
        if let Some(ref worker) = rt.worker {
            let pid = crate::io::health_store::get_health_u32(&health_state, worker, "pid");
            if pid > 0
                && mando_cc::is_process_alive(pid)
                && mando_cc::kill_process(pid).await.is_ok()
            {
                killed += 1;
            }
        }
    }
    Ok(killed)
}

/// Transition an item to CaptainMerging. The captain tick will spawn an AI
/// session to check CI, trigger it if needed, and merge when green.
pub async fn merge_pr(store: &TaskStore, pr_number: &str, repo: &str) -> Result<serde_json::Value> {
    // Find the item whose PR URL matches this repo and PR number.
    let pr_suffix = format!("/pull/{pr_number}");
    let items = store.load_all().await?;
    let item = items
        .iter()
        .find(|it| {
            it.pr
                .as_deref()
                .is_some_and(|url| url.contains(repo) && url.ends_with(&pr_suffix))
        })
        .ok_or_else(|| anyhow::anyhow!("no task found for PR #{pr_number} in {repo}"))?;

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
                "retry_count": 0,
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
) -> Result<()> {
    for id in ids {
        update_task(store, *id, &updates).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::answer_clarification;
    use crate::io::task_store::TaskStore;
    use mando_types::task::{ItemStatus, Task};

    async fn test_store() -> TaskStore {
        let db = mando_db::Db::open_in_memory().await.unwrap();
        TaskStore::new(db.pool().clone())
    }

    #[tokio::test]
    async fn answer_clarification_trims_and_persists_human_answer() {
        let store = test_store().await;
        let mut task = Task::new("Needs help");
        task.status = ItemStatus::NeedsClarification;
        task.context = Some("Existing context".into());
        let id = store.add(task).await.unwrap();

        answer_clarification(&store, id, "  Need more logs  ")
            .await
            .unwrap();

        let updated = store.find_by_id(id).await.unwrap().unwrap();
        assert_eq!(updated.status, ItemStatus::Clarifying);
        assert_eq!(
            updated.context.as_deref(),
            Some("Existing context\n\n[Human answer] Need more logs")
        );
    }

    #[tokio::test]
    async fn answer_clarification_rejects_whitespace_only_answers() {
        let store = test_store().await;
        let mut task = Task::new("Needs help");
        task.status = ItemStatus::NeedsClarification;
        task.context = Some("Existing context".into());
        let id = store.add(task).await.unwrap();

        let err = answer_clarification(&store, id, "   ").await.unwrap_err();
        assert!(err.to_string().contains("answer text must not be empty"));

        let unchanged = store.find_by_id(id).await.unwrap().unwrap();
        assert_eq!(unchanged.status, ItemStatus::NeedsClarification);
        assert_eq!(unchanged.context.as_deref(), Some("Existing context"));
    }
}
