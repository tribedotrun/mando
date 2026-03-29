//! Worker spawn helper and tick-result defaults.

use std::collections::HashMap;

use anyhow::Result;
use mando_config::settings::Config;
use mando_config::workflow::CaptainWorkflow;
use mando_types::captain::TickResult;

pub struct ItemSpawnResult {
    pub session_name: String,
    pub session_id: String,
    pub branch: String,
    pub worktree: String,
    pub started_at: String,
}

pub async fn spawn_worker_for_item(
    config: &Config,
    item: &mando_types::Task,
    workflow: &CaptainWorkflow,
    pool: &sqlx::SqlitePool,
) -> Result<ItemSpawnResult> {
    let project_slug = item.project.as_deref();
    let (slug, project_config) = mando_config::resolve_project_config(project_slug, config)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "no project config for '{}'",
                project_slug.unwrap_or("<none>")
            )
        })?;

    let claude_path = crate::io::process_manager::resolve_claude_binary();
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

    Ok(ItemSpawnResult {
        session_name: result.session_name,
        session_id: result.session_id,
        branch: result.branch,
        worktree: result.worktree,
        started_at: now,
    })
}

pub(crate) fn default_tick_result() -> TickResult {
    TickResult {
        mode: String::new(),
        tick_id: None,
        max_workers: 0,
        active_workers: 0,
        tasks: HashMap::new(),
        alerts: Vec::new(),
        dry_actions: Vec::new(),
        error: None,
    }
}
