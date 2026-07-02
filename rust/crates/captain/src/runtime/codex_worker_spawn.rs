use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde_json::json;
use settings::{CaptainWorkflow, ProjectConfig};

use super::codex_app_server::{start_codex_turn, watch_codex_turn, CodexOutputMode};
use super::codex_stream::{append_jsonl, CodexStreamLine};
use crate::io::{hooks, pid_registry};
use crate::Task;

#[tracing::instrument(skip(project_config, item, workflow, pool), fields(task_id = item.id, provider = "codex"))]
pub(super) async fn spawn_worker(
    project_config: &ProjectConfig,
    item: &Task,
    workflow: &CaptainWorkflow,
    pool: &sqlx::SqlitePool,
    agent_config: &settings::AgentConfig,
) -> Result<super::spawner::SpawnResult> {
    let (branch, wt_path) = resolve_worker_checkout(project_config, item).await?;

    let session_name = format!("worker-{}-{}", item.id, item.worker_seq);
    let hook_env = HashMap::new();
    hooks::pre_spawn(&project_config.hooks, &wt_path, &hook_env).await?;

    let prompt = super::codex_worker_prompt::render_worker_prompt(
        item,
        &branch,
        &wt_path,
        project_config,
        workflow,
    )
    .await?;
    let mut draft_pr_number = None;
    if !item.no_pr {
        match super::spawner_pr::create_draft_pr(item, &branch, &wt_path).await {
            Ok(pr_num) => {
                draft_pr_number = Some(pr_num);
                tracing::info!(
                    module = "codex_worker_spawn",
                    task_id = item.id,
                    pr_number = pr_num,
                    "created draft PR before Codex worker spawn"
                );
            }
            Err(e) => {
                tracing::warn!(
                    module = "codex_worker_spawn",
                    task_id = item.id,
                    error = %e,
                    "failed to create draft PR before Codex worker spawn; starting worker without it"
                );
            }
        }
    }

    let started =
        start_codex_turn(&wt_path, &prompt, None, CodexOutputMode::Text, agent_config).await?;
    let stream_path =
        global_infra::paths::codex_derived_stream_path_for_session(&started.thread_id);
    let setup_result: Result<()> = async {
        append_jsonl(
            &stream_path,
            CodexStreamLine(json!({
                "type": "system",
                "subtype": "init",
                "session_id": &started.thread_id,
                "provider": "codex",
                "cwd": wt_path.display().to_string(),
            })),
        )
        .await?;
        append_jsonl(
            &stream_path,
            CodexStreamLine(json!({
                "type": "user",
                "message": {"content": [{"type": "text", "text": prompt}]},
            })),
        )
        .await?;

        pid_registry::register(&started.thread_id, started.pid)?;
        sessions_db::upsert_session(
            pool,
            &sessions_db::SessionUpsert {
                provider: api_types::TaskProvider::Codex,
                session_id: &started.thread_id,
                created_at: &global_types::now_rfc3339(),
                caller: "worker",
                cwd: &wt_path.display().to_string(),
                model: &started.model,
                status: global_types::SessionStatus::Running,
                cost_usd: None,
                duration_ms: None,
                resumed: false,
                task_id: Some(item.id),
                scout_item_id: None,
                worker_name: Some(&session_name),
                resumed_at: None,
                credential_id: None,
                error: None,
                api_error_status: None,
            },
        )
        .await?;

        global_claude::write_stream_meta_at(
            &global_infra::paths::codex_derived_stream_meta_path_for_session(&started.thread_id),
            &global_claude::SessionMeta {
                session_id: &started.thread_id,
                caller: "worker",
                task_id: &item.id.to_string(),
                worker_name: &session_name,
                project: &item.project,
                cwd: &wt_path.display().to_string(),
            },
            "running",
        );
        Ok(())
    }
    .await;
    if let Err(e) = setup_result {
        super::codex_app_server::abort_started_turn(started, None, "worker setup failed").await;
        return Err(e);
    }

    let session_id = started.thread_id.clone();
    let pid = started.pid;
    watch_codex_turn(started, pool.clone(), stream_path.clone());

    Ok(super::spawner::SpawnResult {
        session_name,
        session_id,
        pid,
        branch,
        worktree: wt_path.to_string_lossy().into_owned(),
        stream_path,
        plan: item.plan.clone(),
        pr_number: draft_pr_number,
    })
}

async fn resolve_worker_checkout(
    project_config: &ProjectConfig,
    item: &Task,
) -> Result<(String, PathBuf)> {
    let repo_path = global_infra::paths::expand_tilde(&project_config.path);
    global_git::fetch_origin(&repo_path).await?;
    let stored_wt = item
        .worktree
        .as_deref()
        .map(global_infra::paths::expand_tilde);
    let default_branch = global_git::default_branch(&repo_path).await?;

    match super::spawner::plan_worktree(stored_wt.as_ref(), item.branch.as_deref()) {
        super::spawner::WorktreePlan::Reuse { wt, branch } => {
            tracing::info!(
                module = "codex_worker_spawn",
                worktree = %wt.display(),
                branch = %branch,
                "reusing existing worktree for Codex worker"
            );
            Ok((branch, wt))
        }
        super::spawner::WorktreePlan::Rework { wt } => {
            let slug = new_slug(
                item,
                super::task_creation::next_worker_slot(&global_infra::paths::state_dir())?,
            );
            let branch = format!("mando/{}", slug);
            tracing::info!(
                module = "codex_worker_spawn",
                worktree = %wt.display(),
                branch = %branch,
                "resetting existing worktree to a fresh Codex worker branch"
            );
            global_git::reset_to_new_branch(&wt, &branch, &default_branch).await?;
            Ok((branch, wt))
        }
        super::spawner::WorktreePlan::Recreate { wt, stored_branch } => {
            tracing::warn!(
                module = "codex_worker_spawn",
                task_id = item.id,
                worktree = %wt.display(),
                stored_branch = ?stored_branch,
                "Codex task worktree missing on disk; recreating at stored path"
            );
            let branch = recreate_worktree_at(item, &repo_path, &default_branch, &wt).await?;
            Ok((branch, wt))
        }
        super::spawner::WorktreePlan::MissingBinding => {
            anyhow::bail!(
                "task {} reached Codex spawn without a worktree binding -- refusing to spawn",
                item.id,
            );
        }
    }
}

fn new_slug(item: &Task, slot: u64) -> String {
    format!("todo-{}-{}", item.id, slot)
}

async fn recreate_worktree_at(
    item: &Task,
    repo_path: &Path,
    default_branch: &str,
    wt: &Path,
) -> Result<String> {
    const MAX_ATTEMPTS: usize = 20;

    for attempt in 0..MAX_ATTEMPTS {
        let slug = new_slug(
            item,
            super::task_creation::next_worker_slot(&global_infra::paths::state_dir())?,
        );
        let branch = format!("mando/{}", slug);

        if let Err(e) = global_git::prune_worktrees(repo_path).await {
            tracing::warn!(module = "codex_worker_spawn", error = %e, "failed to prune stale git worktree metadata before recreate");
        }
        if let Err(e) = global_git::delete_local_branch(repo_path, &branch).await {
            tracing::debug!(module = "codex_worker_spawn", branch = %branch, error = %e, "stale branch cleanup before recreate");
        }

        match global_git::create_worktree(repo_path, &branch, wt, default_branch).await {
            Ok(()) => {
                crate::io::worktree_bootstrap::copy_local_files(repo_path, wt).await;
                return Ok(branch);
            }
            Err(e)
                if crate::find_git_error(&e).is_some_and(|g| {
                    matches!(g, crate::GitError::WorktreeAlreadyExists { .. })
                }) && attempt + 1 < MAX_ATTEMPTS =>
            {
                tracing::warn!(
                    module = "codex_worker_spawn",
                    branch = %branch,
                    worktree = %wt.display(),
                    attempt = attempt + 1,
                    "branch/worktree already exists during Codex recreate; rotating slug and retrying"
                );
            }
            Err(e) => return Err(e),
        }
    }

    anyhow::bail!(
        "failed to recreate Codex worktree at {} after {} attempts for task {}",
        wt.display(),
        MAX_ATTEMPTS,
        item.id
    );
}
