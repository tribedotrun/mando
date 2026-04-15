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
    pub workbench_id: i64,
    pub started_at: String,
    /// Worktree-relative path to the plan/brief file, if one was found.
    pub plan: Option<String>,
    pub credential_id: Option<i64>,
    pub pr_number: Option<i64>,
}

/// Pick the best credential via a single DB query: not expired, not
/// rate-limited, fewest active running sessions.
/// Returns `(id, access_token)` or `None` if no credentials are configured.
///
/// `caller_filter` narrows which running sessions count toward load
/// balancing. Pass `Some("worker")` for worker spawns so only other
/// workers influence the pick. Pass `None` to count all sessions.
pub async fn pick_credential(
    pool: &sqlx::SqlitePool,
    caller_filter: Option<&str>,
) -> Option<(i64, String)> {
    match mando_db::queries::credentials::pick_for_worker(pool, caller_filter).await {
        Ok(pick) => pick,
        Err(e) => {
            tracing::warn!(
                module = "credentials",
                error = %e,
                "failed to pick credential"
            );
            None
        }
    }
}

/// Inject credential env var into a CcConfig builder if credential is Some.
/// Returns the builder (consumed and returned).
pub fn with_credential(
    builder: mando_cc::CcConfigBuilder,
    credential: &Option<(i64, String)>,
) -> mando_cc::CcConfigBuilder {
    if let Some((_id, token)) = credential {
        builder.env("CLAUDE_CODE_OAUTH_TOKEN", token)
    } else {
        builder
    }
}

/// Extract credential_id from an Option<(id, token)>.
pub fn credential_id(credential: &Option<(i64, String)>) -> Option<i64> {
    credential.as_ref().map(|c| c.0)
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

    if !item.no_pr && project_config.github_repo.is_none() {
        anyhow::bail!(
            "project '{}' has no githubRepo configured -- cannot process PR-based tasks",
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

    // Pick credential for multi-account load balancing.
    // Workers dominate token spend, so balance on worker sessions only.
    let credential = pick_credential(pool, Some("worker")).await;
    let cred_id = credential.as_ref().map(|c| c.0);
    let worker_cred = credential
        .as_ref()
        .map(|c| super::spawner::WorkerCredential {
            id: c.0,
            token: &c.1,
        });

    let result = super::spawner::spawn_worker(
        item,
        slug,
        project_config,
        &config.captain,
        workflow,
        pool,
        worker_cred.as_ref(),
    )
    .await?;
    let now = mando_types::now_rfc3339();

    // Create a workbench row for this worktree. Use the clarified title
    // (clarification has already run by the time a task reaches spawn).
    let wb_title = &item.title;
    let workbench_id =
        create_workbench_for_spawn(pool, item.project_id, slug, &result.worktree, wb_title).await?;

    // Archive the previous workbench when a rework/redispatch creates a new
    // one. Without this, the old workbench lingers as an orphan in the sidebar.
    if item.workbench_id != 0 && item.workbench_id != workbench_id {
        if let Err(e) = mando_db::queries::workbenches::archive(pool, item.workbench_id).await {
            tracing::warn!(
                module = "captain",
                old_wb = item.workbench_id,
                new_wb = workbench_id,
                error = %e,
                "failed to archive previous workbench"
            );
        }
    }

    Ok(ItemSpawnResult {
        session_name: result.session_name,
        session_id: result.session_id,
        branch: result.branch,
        worktree: result.worktree,
        workbench_id,
        started_at: now,
        plan: result.plan,
        credential_id: cred_id,
        pr_number: result.pr_number,
    })
}

/// Create a workbench row for a newly spawned worker, returning the ID.
/// If a workbench already exists for this worktree path, reuse it.
async fn create_workbench_for_spawn(
    pool: &sqlx::SqlitePool,
    project_id: i64,
    project_slug: &str,
    worktree: &str,
    task_prompt: &str,
) -> Result<i64> {
    // Check if a workbench already exists for this worktree.
    if let Some(existing) = mando_db::queries::workbenches::find_by_worktree(pool, worktree).await?
    {
        return Ok(existing.id);
    }
    let title = if task_prompt.is_empty() {
        mando_types::workbench::workbench_title_now()
    } else {
        task_prompt.to_string()
    };
    let wb = mando_types::Workbench::new(
        project_id,
        project_slug.to_string(),
        worktree.to_string(),
        title,
    );
    let id = mando_db::queries::workbenches::insert(pool, &wb).await?;
    Ok(id)
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
