//! Worker spawn helper and tick-result defaults.

use std::collections::HashMap;

use anyhow::Result;
use mando_config::settings::Config;
use mando_config::workflow::CaptainWorkflow;
use mando_types::captain::{TickMode, TickResult};

pub struct ItemSpawnResult {
    pub session_name: String,
    pub session_id: String,
    pub branch: String,
    pub worktree: String,
    pub workbench_id: Option<i64>,
    pub started_at: String,
    /// Worktree-relative path to the plan/brief file, if one was found.
    pub plan: Option<String>,
}

pub async fn spawn_worker_for_item(
    config: &Config,
    item: &mando_types::Task,
    workflow: &CaptainWorkflow,
    pool: &sqlx::SqlitePool,
) -> Result<ItemSpawnResult> {
    let (slug, project_config) =
        mando_config::resolve_project_config(Some(item.project.as_str()), config)
            .ok_or_else(|| anyhow::anyhow!("no project config for '{}'", item.project))?;

    // GitHub is required for PR-based tasks. Reject upfront instead of
    // discovering the problem mid-work in the nudge loop.
    if !item.no_pr && project_config.github_repo.is_none() {
        anyhow::bail!(
            "project '{}' has no githubRepo configured — cannot process PR-based tasks",
            slug
        );
    }

    let claude_path = mando_cc::resolve_claude_binary();
    if !claude_path.exists() && claude_path.to_str() == Some("claude") {
        let which = tokio::process::Command::new("which")
            .arg("claude")
            .output()
            .await;
        match which {
            Ok(out) if out.status.success() => {}
            _ => {
                anyhow::bail!(
                    "claude binary not found (checked {:?} and PATH)",
                    claude_path
                );
            }
        }
    }

    let result =
        super::spawner::spawn_worker(item, slug, project_config, &config.captain, workflow, pool)
            .await?;
    let now = mando_types::now_rfc3339();

    // Create a workbench row for this worktree.
    let workbench_id =
        create_workbench_for_spawn(pool, item.project_id, slug, &result.worktree, item.id).await?;

    Ok(ItemSpawnResult {
        session_name: result.session_name,
        session_id: result.session_id,
        branch: result.branch,
        worktree: result.worktree,
        workbench_id,
        started_at: now,
        plan: result.plan,
    })
}

/// Create a workbench row for a newly spawned worker, returning the ID.
/// If a workbench already exists for this worktree path, reuse it.
async fn create_workbench_for_spawn(
    pool: &sqlx::SqlitePool,
    project_id: i64,
    project_slug: &str,
    worktree: &str,
    _task_id: i64,
) -> Result<Option<i64>> {
    // Check if a workbench already exists for this worktree.
    if let Some(existing) = mando_db::queries::workbenches::find_by_worktree(pool, worktree).await?
    {
        return Ok(Some(existing.id));
    }
    let title = mando_types::workbench::workbench_title_now();
    let wb = mando_types::Workbench::new(
        project_id,
        project_slug.to_string(),
        worktree.to_string(),
        title,
    );
    let id = mando_db::queries::workbenches::insert(pool, &wb).await?;
    Ok(Some(id))
}

pub(crate) fn default_tick_result() -> TickResult {
    TickResult {
        mode: TickMode::Skipped,
        tick_id: None,
        max_workers: 0,
        active_workers: 0,
        tasks: HashMap::new(),
        alerts: Vec::new(),
        dry_actions: Vec::new(),
        error: None,
        rate_limited: false,
    }
}
