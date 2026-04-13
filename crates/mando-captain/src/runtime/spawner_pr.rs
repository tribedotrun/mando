//! Draft PR creation for the spawner -- scaffold commit, push, gh pr create.

use anyhow::{Context, Result};
use mando_types::Task;

/// Create a draft PR for a task: scaffold commit, push, gh pr create --draft.
/// Returns the PR number. Updates the task's pr_number in DB.
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
    let git_commit = tokio::process::Command::new("git")
        .args([
            "-C",
            &wt_str,
            "commit",
            "--allow-empty",
            "-m",
            &format!("chore: scaffold task #{}", item.id),
        ])
        .output()
        .await?;
    if !git_commit.status.success() {
        let stderr = String::from_utf8_lossy(&git_commit.stderr);
        if !stderr.contains("nothing to commit") {
            anyhow::bail!("git commit scaffold failed: {stderr}");
        }
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
    let body = format!(
        "## Problem\n\n{problem}\n\n\
         ## Solution\n\n<!-- work-summary: pending -->\n\n\
         ## Evidence\n\n<!-- evidence: pending -->\n\n\
         ## Reviewer Checklist\n\n\
         - [ ] DB migration\n\
         - [ ] Env vars\n\
         - [ ] New dependencies\n\
         - [ ] Backend deploy\n\
         - [ ] Breaking changes\n\
         - [ ] External API calls\n\
         - [ ] No backward-compat / legacy code\n\
         - [ ] Wiring completeness\n\
         - [ ] Electron UI surfacing\n\n\
         ## Testing & Verification\n\n\
         ### Unit tests\n<!-- filled by worker -->\n\n\
         ### E2E regression\n<!-- filled by worker -->\n\n\
         ### E2E verification\n<!-- filled by worker -->\n"
    );

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

    sqlx::query("UPDATE tasks SET pr_number = ?1 WHERE id = ?2")
        .bind(pr_num)
        .bind(item.id)
        .execute(pool)
        .await?;

    Ok(pr_num)
}
