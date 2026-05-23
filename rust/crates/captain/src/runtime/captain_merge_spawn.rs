//! Spawn logic for captain merge sessions.

use anyhow::Result;
use rustc_hash::FxHashMap;

use settings::CaptainWorkflow;
use settings::Config;

use crate::Task;

use super::notify::Notifier;

/// Spawn a captain merge session for an item. Sets status to CaptainMerging.
#[tracing::instrument(skip_all)]
pub(crate) async fn spawn_merge(
    item: &mut Task,
    config: &Config,
    workflow: &CaptainWorkflow,
    notifier: &Notifier,
    pool: &sqlx::SqlitePool,
) -> Result<()> {
    let cwd = item
        .worktree
        .as_deref()
        .map(std::path::PathBuf::from)
        .or_else(|| {
            config
                .captain
                .projects
                .values()
                .next()
                .map(|p| std::path::PathBuf::from(&p.path))
        })
        .ok_or_else(|| {
            anyhow::anyhow!(
                "no CWD for captain merge: item has no worktree and no projects configured"
            )
        })?;

    let pr_num = item
        .pr_number
        .ok_or_else(|| anyhow::anyhow!("cannot merge item without a PR"))?;

    let pr_number = pr_num.to_string();

    let repo = item
        .github_repo
        .clone()
        .or_else(|| settings::resolve_github_repo(Some(&item.project), config))
        .ok_or_else(|| anyhow::anyhow!("no github_repo for project {:?}", item.project))?;

    let pr_url = format!("https://github.com/{repo}/pull/{pr_number}");

    // Render prompt before any side effects so failures propagate as Err
    // rather than dying silently inside tokio::spawn.
    let mut vars: FxHashMap<&str, &str> = FxHashMap::default();
    vars.insert("pr_url", pr_url.as_str());
    vars.insert("repo", repo.as_str());
    vars.insert("pr_number", pr_number.as_str());
    vars.insert("title", item.title.as_str());
    let prompt = settings::render_prompt("captain_merge", &workflow.prompts, &vars)
        .map_err(|e| anyhow::anyhow!("render captain_merge prompt: {e}"))?;

    super::agent_runtime::spawn_merge_session(
        item, &cwd, notifier, pool, &pr_url, &pr_number, &prompt, workflow,
    )
    .await
}
