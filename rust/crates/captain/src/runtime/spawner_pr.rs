//! Draft PR creation for the spawner -- scaffold commit, push, GitHub PR create.

use crate::Task;
use anyhow::Result;

/// Create a draft PR for a task: scaffold commit, push, gh pr create --draft.
/// Returns the PR number; persistence is the caller's responsibility.
#[tracing::instrument(skip_all)]
pub(crate) async fn create_draft_pr(
    item: &Task,
    branch: &str,
    wt_path: &std::path::Path,
) -> Result<i64> {
    let briefs_dir = wt_path.join(".ai").join("briefs");
    std::fs::create_dir_all(&briefs_dir)?;
    let scaffold_file = briefs_dir.join(".gitkeep");
    if !scaffold_file.exists() {
        std::fs::write(&scaffold_file, "")?;
    }

    // Force-add: .ai/ may be in .gitignore for some projects.
    global_git::add_force(wt_path, ".ai/briefs/.gitkeep").await?;

    // Swallow `GitError::NothingToCommit` since `--allow-empty` should ensure
    // the commit succeeds even with no staged changes; the carve-out stays
    // defensive for future git versions that diverge.
    let message = format!("chore: scaffold task #{}", item.id);
    match global_git::commit_allow_empty(wt_path, &message).await {
        Ok(()) => {}
        Err(e) => match crate::find_git_error(&e) {
            Some(crate::GitError::NothingToCommit) => {}
            _ => return Err(e),
        },
    }

    // Force-push: the branch may exist remotely from a previous attempt
    // (e.g. stale sandbox data or retried spawn after failure).
    global_git::push_set_upstream_force(wt_path, "origin", branch).await?;

    let context = item.context.as_deref().unwrap_or("");
    let original_prompt = item.original_prompt.as_deref().unwrap_or("");
    let problem = format!("{}\n\n{}\n\n{}", item.title, context, original_prompt);
    let body = format!("## Problem\n\n{problem}\n");
    let pr_num = global_github::create_draft_pr(wt_path, &item.title, &body).await?;

    Ok(pr_num)
}
