use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::Result;
use global_opencode::OpenCodeRunConfig;
use settings::{workflow::StageAgentConfig, CaptainWorkflow, ProjectConfig};

use crate::io::{hooks, pid_registry};
use crate::{Pid, Task};

#[tracing::instrument(skip(project_config, item, workflow, pool), fields(task_id = item.id, provider = "opencode"))]
pub(super) async fn spawn_worker(
    project_config: &ProjectConfig,
    item: &Task,
    workflow: &CaptainWorkflow,
    pool: &sqlx::SqlitePool,
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
    let pr_number = maybe_create_draft_pr(item, &branch, &wt_path).await;
    let started = start_opencode_worker(&StartOpenCodeWorker {
        pool,
        item,
        worker_name: &session_name,
        cwd: &wt_path,
        prompt: &prompt,
        resume_session_id: None,
        stage: workflow
            .stages
            .require(settings::WorkflowStage::Implementation),
        resumed: false,
    })
    .await?;

    Ok(super::spawner::SpawnResult {
        session_name,
        session_id: started.session_id,
        pid: started.pid,
        branch,
        worktree: wt_path.to_string_lossy().into_owned(),
        stream_path: started.stream_path,
        plan: item.plan.clone(),
        pr_number,
    })
}

pub(super) struct OpenCodeWorkerResume {
    pub(super) pid: Pid,
    pub(super) session_id: String,
}

#[tracing::instrument(skip(pool, item, prompt, stage), fields(task_id = item.id, session_id, provider = "opencode"))]
pub(super) async fn resume_worker(
    pool: &sqlx::SqlitePool,
    item: &Task,
    worker_name: &str,
    cwd: &Path,
    prompt: &str,
    session_id: &str,
    stage: &StageAgentConfig,
) -> Result<OpenCodeWorkerResume> {
    kill_existing_worker(session_id, worker_name).await;
    let started = start_opencode_worker(&StartOpenCodeWorker {
        pool,
        item,
        worker_name,
        cwd,
        prompt,
        resume_session_id: Some(session_id),
        stage,
        resumed: true,
    })
    .await?;
    Ok(OpenCodeWorkerResume {
        pid: started.pid,
        session_id: started.session_id,
    })
}

#[tracing::instrument(skip_all, fields(provider = "opencode", session_id))]
pub(super) async fn terminate_worker_process(session_id: &str) -> Result<()> {
    if let Some(pid) = crate::io::pid_registry::get_verified_pid(session_id) {
        global_opencode::terminate_process(pid).await?;
    }
    if let Err(e) = pid_registry::unregister(session_id) {
        tracing::warn!(module = "opencode_worker_spawn", session_id, error = %e, "failed to unregister OpenCode pid after terminate");
    }
    Ok(())
}

struct StartedOpenCodeWorker {
    session_id: String,
    pid: Pid,
    stream_path: PathBuf,
}

struct StartOpenCodeWorker<'a> {
    pool: &'a sqlx::SqlitePool,
    item: &'a Task,
    worker_name: &'a str,
    cwd: &'a Path,
    prompt: &'a str,
    resume_session_id: Option<&'a str>,
    stage: &'a StageAgentConfig,
    resumed: bool,
}

struct RegisteredOpenCodePid {
    session_id: String,
    identity: pid_registry::PidEntry,
    cleanup_on_drop: bool,
}

impl RegisteredOpenCodePid {
    fn register(session_id: &str, pid: Pid) -> Result<Self> {
        let identity = pid_registry::register(session_id, pid)?;
        Ok(Self {
            session_id: session_id.to_string(),
            identity,
            cleanup_on_drop: true,
        })
    }

    fn identity(&self) -> &pid_registry::PidEntry {
        &self.identity
    }

    fn keep_registered(mut self) {
        self.cleanup_on_drop = false;
    }
}

impl Drop for RegisteredOpenCodePid {
    fn drop(&mut self) {
        if !self.cleanup_on_drop {
            return;
        }
        if let Err(e) = pid_registry::unregister_entry_if_current(&self.session_id, &self.identity)
        {
            tracing::warn!(
                module = "opencode_worker_spawn",
                session_id = %self.session_id,
                pid = %self.identity.pid,
                error = %e,
                "failed to unregister OpenCode pid after startup setup failure"
            );
        }
    }
}

async fn start_opencode_worker(request: &StartOpenCodeWorker<'_>) -> Result<StartedOpenCodeWorker> {
    let started_run = global_opencode::spawn_run(
        &OpenCodeRunConfig {
            cwd: request.cwd,
            prompt: request.prompt,
            model: &request.stage.model,
            variant: request.stage.variant.as_deref(),
            resume_session_id: request.resume_session_id,
        },
        request.stage.session_start_timeout_s,
    )
    .await?;
    let session_id = started_run.session_id.clone();
    let stream_path = global_infra::paths::opencode_stream_path_for_session(&session_id);
    let registration = RegisteredOpenCodePid::register(&session_id, started_run.pid)?;
    super::opencode_worker_stream::write_initial_stream(
        &stream_path,
        &session_id,
        request.cwd,
        request.prompt,
    )
    .await?;
    let mut stream_state = global_opencode::OpenCodeStreamState::default();
    for event in &started_run.buffered_events {
        super::opencode_worker_stream::append_opencode_event(
            &stream_path,
            event,
            &mut stream_state,
        )
        .await?;
    }

    let resumed_at = request.resumed.then(global_types::now_rfc3339);
    sessions_db::upsert_session(
        request.pool,
        &sessions_db::SessionUpsert {
            provider: api_types::TaskProvider::OpenCode,
            session_id: &session_id,
            created_at: &global_types::now_rfc3339(),
            caller: "worker",
            cwd: &request.cwd.display().to_string(),
            model: &request.stage.model,
            status: global_types::SessionStatus::Running,
            cost_usd: None,
            duration_ms: None,
            resumed: request.resumed,
            task_id: Some(request.item.id),
            scout_item_id: None,
            worker_name: Some(request.worker_name),
            resumed_at: resumed_at.as_deref(),
            credential_id: None,
            error: None,
            api_error_status: None,
        },
    )
    .await?;
    global_claude::write_stream_meta_at(
        &global_infra::paths::opencode_stream_meta_path_for_session(&session_id),
        &global_claude::SessionMeta {
            session_id: &session_id,
            caller: "worker",
            task_id: &request.item.id.to_string(),
            worker_name: request.worker_name,
            project: &request.item.project,
            cwd: &request.cwd.display().to_string(),
        },
        "running",
    );

    let pid = started_run.pid;
    let pid_identity = registration.identity().clone();
    super::opencode_worker_stream::watch_opencode_worker(
        started_run,
        request.pool.clone(),
        session_id.clone(),
        stream_path.clone(),
        stream_state,
        pid_identity,
    );
    registration.keep_registered();

    Ok(StartedOpenCodeWorker {
        session_id,
        pid,
        stream_path,
    })
}

async fn kill_existing_worker(session_id: &str, worker_name: &str) {
    if let Some(pid) = crate::io::pid_lookup::resolve_pid(session_id, worker_name) {
        if pid.as_u32() > 0 {
            if let Err(e) = global_opencode::terminate_process(pid).await {
                tracing::warn!(
                    module = "opencode_worker_spawn",
                    worker = %worker_name,
                    pid = %pid,
                    error = %e,
                    "failed to kill old OpenCode process before resume"
                );
            }
            if let Err(e) = pid_registry::unregister(session_id) {
                tracing::warn!(module = "opencode_worker_spawn", session_id, error = %e, "failed to unregister old OpenCode pid before resume");
            }
        }
    }
}

async fn maybe_create_draft_pr(item: &Task, branch: &str, wt_path: &Path) -> Option<i64> {
    if item.no_pr {
        return None;
    }
    match super::spawner_pr::create_draft_pr(item, branch, wt_path).await {
        Ok(pr_num) => Some(pr_num),
        Err(e) => {
            tracing::warn!(
                module = "opencode_worker_spawn",
                task_id = item.id,
                error = %e,
                "failed to create draft PR before OpenCode worker spawn; starting worker without it"
            );
            None
        }
    }
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
        super::spawner::WorktreePlan::Reuse { wt, branch } => Ok((branch, wt)),
        super::spawner::WorktreePlan::Rework { wt } => {
            let slug = new_slug(
                item,
                super::task_creation::next_worker_slot(&global_infra::paths::state_dir())?,
            );
            let branch = format!("mando/{}", slug);
            global_git::reset_to_new_branch(&wt, &branch, &default_branch).await?;
            Ok((branch, wt))
        }
        super::spawner::WorktreePlan::Recreate {
            wt,
            stored_branch: _,
        } => {
            let branch = recreate_worktree_at(item, &repo_path, &default_branch, &wt).await?;
            Ok((branch, wt))
        }
        super::spawner::WorktreePlan::MissingBinding => {
            anyhow::bail!(
                "task {} reached OpenCode spawn without a worktree binding -- refusing to spawn",
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
            tracing::warn!(module = "opencode_worker_spawn", error = %e, "failed to prune stale git worktree metadata before recreate");
        }
        if let Err(e) = global_git::delete_local_branch(repo_path, &branch).await {
            tracing::debug!(module = "opencode_worker_spawn", branch = %branch, error = %e, "stale branch cleanup before recreate");
        }
        match global_git::create_worktree(repo_path, &branch, wt, default_branch).await {
            Ok(()) => {
                crate::io::worktree_bootstrap::copy_local_files(repo_path, wt).await;
                return Ok(branch);
            }
            Err(e)
                if crate::find_git_error(&e).is_some_and(|g| {
                    matches!(g, crate::GitError::WorktreeAlreadyExists { .. })
                }) && attempt + 1 < MAX_ATTEMPTS => {}
            Err(e) => return Err(e),
        }
    }

    anyhow::bail!(
        "failed to recreate OpenCode worktree at {} after {} attempts for task {}",
        wt.display(),
        MAX_ATTEMPTS,
        item.id
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn isolate_data_dir() -> (std::path::PathBuf, global_infra::EnvVarGuard) {
        let dir = std::env::temp_dir().join(format!(
            "mando-opencode-spawn-{}",
            global_infra::uuid::Uuid::v4()
        ));
        std::fs::create_dir_all(dir.join("state")).unwrap();
        let guard = global_infra::EnvVarGuard::set("MANDO_DATA_DIR", &dir);
        (dir, guard)
    }

    #[tokio::test]
    async fn registered_pid_guard_cleans_up_on_setup_failure_drop() {
        let _lock = global_infra::PROCESS_ENV_LOCK.lock().await;
        let (_dir, _guard) = isolate_data_dir();
        let session_id = "ses-opencode-startup-failure";
        let pid = Pid::new(999_999_999);

        {
            let _registration = RegisteredOpenCodePid::register(session_id, pid).unwrap();
            assert_eq!(pid_registry::get_pid(session_id), Some(pid));
        }

        assert_eq!(pid_registry::get_pid(session_id), None);
    }
}
