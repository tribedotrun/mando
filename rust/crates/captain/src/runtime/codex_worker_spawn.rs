use std::collections::HashMap;

use anyhow::Result;
use settings::{CaptainWorkflow, ProjectConfig};

use super::codex_app_server::{start_codex_turn, CodexOutputMode};
use super::codex_session::{begin_codex_session, CodexSessionSpec};
use crate::io::hooks;
use crate::Task;

#[tracing::instrument(skip(project_config, item, workflow, pool), fields(task_id = item.id, provider = "codex"))]
pub(super) async fn spawn_worker(
    project_config: &ProjectConfig,
    item: &Task,
    workflow: &CaptainWorkflow,
    pool: &sqlx::SqlitePool,
    agent_config: &settings::AgentConfig,
) -> Result<super::spawner::SpawnResult> {
    let (branch, wt_path) =
        super::worker_checkout::resolve_worker_checkout(project_config, item, "codex_worker_spawn")
            .await?;

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
    let session = begin_codex_session(
        pool,
        started,
        CodexSessionSpec {
            caller: "worker",
            task_id: item.id,
            project: &item.project,
            worker_name: Some(&session_name),
            cwd: &wt_path,
            prompt: &prompt,
            resumed: false,
            alias: None,
            abort_reason: "worker setup failed",
        },
    )
    .await?;

    Ok(super::spawner::SpawnResult {
        session_name,
        session_id: session.session_id,
        branch,
        worktree: wt_path.to_string_lossy().into_owned(),
        plan: item.plan.clone(),
        pr_number: draft_pr_number,
    })
}
