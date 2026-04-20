//! Draft PR creation for the spawner -- scaffold commit, push, gh pr create.

use crate::Task;
use anyhow::{Context, Result};

/// Create a draft PR for a task: scaffold commit, push, gh pr create --draft.
/// Returns the PR number. Updates the task's pr_number in DB.
#[tracing::instrument(skip_all)]
pub(crate) async fn create_draft_pr(
    item: &Task,
    branch: &str,
    wt_path: &std::path::Path,
    pool: &sqlx::SqlitePool,
) -> Result<i64> {
    let briefs_dir = wt_path.join(".ai").join("briefs");
    std::fs::create_dir_all(&briefs_dir)?;
    let scaffold_file = briefs_dir.join(".gitkeep");
    if !scaffold_file.exists() {
        std::fs::write(&scaffold_file, "")?;
    }

    let wt_str = wt_path.to_string_lossy();
    // Force-add: .ai/ may be in .gitignore for some projects.
    let git_add = tokio::process::Command::new("git")
        .args(["-C", &wt_str, "add", "-f", ".ai/briefs/.gitkeep"])
        .output()
        .await?;
    if !git_add.status.success() {
        anyhow::bail!(
            "git add scaffold failed: {}",
            String::from_utf8_lossy(&git_add.stderr)
        );
    }
    // Swallow `GitError::NothingToCommit` since `--allow-empty` should ensure
    // the commit succeeds even with no staged changes; the carve-out stays
    // defensive for future git versions that diverge.
    match scaffold_commit(&wt_str, item.id).await {
        Ok(()) => {}
        Err(e) => match crate::find_git_error(&e) {
            Some(crate::GitError::NothingToCommit) => {}
            _ => return Err(e),
        },
    }

    // Force-push: the branch may exist remotely from a previous attempt
    // (e.g. stale sandbox data or retried spawn after failure).
    let git_push = tokio::process::Command::new("git")
        .args(["-C", &wt_str, "push", "-u", "--force", "origin", branch])
        .output()
        .await?;
    if !git_push.status.success() {
        anyhow::bail!(
            "git push failed: {}",
            String::from_utf8_lossy(&git_push.stderr)
        );
    }

    let context = item.context.as_deref().unwrap_or("");
    let original_prompt = item.original_prompt.as_deref().unwrap_or("");
    let problem = format!("{}\n\n{}\n\n{}", item.title, context, original_prompt);
    let body = format!("## Problem\n\n{problem}\n");

    let gh_output = tokio::process::Command::new("gh")
        .args([
            "pr",
            "create",
            "--draft",
            "--title",
            &item.title,
            "--body",
            &body,
        ])
        .current_dir(wt_path)
        .output()
        .await?;
    if !gh_output.status.success() {
        anyhow::bail!(
            "gh pr create failed: {}",
            String::from_utf8_lossy(&gh_output.stderr)
        );
    }

    let url = String::from_utf8_lossy(&gh_output.stdout)
        .trim()
        .to_string();
    let pr_num: i64 = url
        .rsplit('/')
        .next()
        .and_then(|s| s.parse().ok())
        .context("failed to parse PR number from gh output")?;

    crate::io::queries::tasks_persist::set_pr_number(pool, item.id, pr_num).await?;

    Ok(pr_num)
}

/// Run the scaffold commit with `--allow-empty`, raising a typed
/// [`crate::GitError::NothingToCommit`] when git rejects with that stderr
/// so callers can discriminate without substring matching.
async fn scaffold_commit(wt_str: &str, task_id: i64) -> Result<()> {
    let output = tokio::process::Command::new("git")
        .args([
            "-C",
            wt_str,
            "commit",
            "--allow-empty",
            "-m",
            &format!("chore: scaffold task #{task_id}"),
        ])
        .output()
        .await?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let trimmed = stderr.trim();
    if trimmed.contains("nothing to commit") {
        return Err(crate::GitError::NothingToCommit.into());
    }
    anyhow::bail!("git commit scaffold failed: {trimmed}");
}
