//! Git worktree, branch, and rebase operations.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Create a git worktree with a new branch.
///
/// After `git worktree add`, copies gitignored local-only files that are
/// needed per-worktree (e.g. `CLAUDE.local.md`, `devtools/mando-dev/dev.env.local`).
pub async fn create_worktree(
    repo_path: &Path,
    branch: &str,
    wt_path: &Path,
    base_ref: &str,
) -> Result<()> {
    let wt_str = wt_path
        .to_str()
        .context("worktree path is not valid UTF-8")?;
    run_git(
        repo_path,
        &["worktree", "add", "-b", branch, wt_str, base_ref],
    )
    .await?;

    // Copy gitignored local-only files that workers need.
    let local_files: &[&str] = &["claude.local.md", "devtools/mando-dev/dev.env.local"];
    for rel in local_files {
        let src = repo_path.join(rel);
        if src.exists() {
            let dst = wt_path.join(rel);
            if let Some(parent) = dst.parent() {
                tokio::fs::create_dir_all(parent).await.ok();
            }
            if let Err(e) = tokio::fs::copy(&src, &dst).await {
                tracing::warn!(
                    module = "git",
                    file = %rel,
                    error = %e,
                    "failed to copy local file into worktree"
                );
            }
        }
    }

    Ok(())
}

/// Remove a git worktree. Falls back to rm -rf if git remove fails.
/// Returns error only if both methods fail.
pub async fn remove_worktree(repo_path: &Path, wt_path: &Path) -> Result<()> {
    let wt_str = wt_path
        .to_str()
        .context("worktree path is not valid UTF-8")?;
    let output = tokio::process::Command::new("git")
        .args(["worktree", "remove", "--force"])
        .arg(wt_path)
        .current_dir(repo_path)
        .output()
        .await
        .context("git worktree remove")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        tracing::warn!(
            module = "git",
            path = %wt_path.display(),
            stderr = %stderr.trim(),
            "git worktree remove failed, falling back to rm -rf"
        );
        tokio::fs::remove_dir_all(wt_path)
            .await
            .with_context(|| format!("rm -rf also failed for {}", wt_str))?;
    }
    Ok(())
}

/// Delete a local branch.
pub async fn delete_local_branch(repo_path: &Path, branch: &str) -> Result<()> {
    run_git(repo_path, &["branch", "-D", branch]).await?;
    Ok(())
}

/// Prune stale git worktree metadata for worktrees whose directories no longer exist.
pub async fn prune_worktrees(repo_path: &Path) -> Result<()> {
    run_git(repo_path, &["worktree", "prune"]).await?;
    Ok(())
}

/// Fetch from origin so local refs are up to date.
pub async fn fetch_origin(repo_path: &Path) -> Result<()> {
    run_git(repo_path, &["fetch", "origin"]).await?;
    Ok(())
}

/// Get the default branch name (main, master, develop, trunk, etc.).
/// Returns Err if the git command fails or the result is empty. Does not
/// fall back to a hardcoded branch name; callers must handle failure.
pub async fn default_branch(repo_path: &Path) -> Result<String> {
    let text = run_git(
        repo_path,
        &["symbolic-ref", "refs/remotes/origin/HEAD", "--short"],
    )
    .await?;
    if text.is_empty() {
        anyhow::bail!(
            "git symbolic-ref returned empty output for origin/HEAD in {}",
            repo_path.display()
        );
    }
    Ok(text)
}

/// List all worktree paths for a repo.
pub async fn list_worktrees(repo_path: &Path) -> Result<Vec<PathBuf>> {
    let text = run_git(repo_path, &["worktree", "list", "--porcelain"]).await?;
    let mut paths = Vec::new();
    for line in text.lines() {
        if let Some(path) = line.strip_prefix("worktree ") {
            paths.push(PathBuf::from(path));
        }
    }
    Ok(paths)
}

/// Central worktrees directory (`~/.mando/worktrees` or `$MANDO_DATA_DIR/worktrees`).
pub fn worktrees_dir() -> PathBuf {
    mando_types::data_dir().join("worktrees")
}

/// Compute worktree path: `<worktrees_dir>/<repo_name>-<slug>`.
pub fn worktree_path(repo_path: &Path, slug: &str) -> PathBuf {
    let repo_name = repo_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("repo");
    worktrees_dir().join(format!("{}-{}", repo_name, slug))
}

/// Run a git command and return stdout. Fails on non-zero exit.
pub(crate) async fn run_git(cwd: &Path, args: &[&str]) -> Result<String> {
    let output = tokio::process::Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .await
        .with_context(|| format!("git {}", args.first().unwrap_or(&"?")))?;

    if !output.status.success() {
        // Stderr is display-only for the error message, so lossy is fine here.
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "git {} failed: {}",
            args.first().unwrap_or(&"?"),
            stderr.trim()
        );
    }
    let stdout = String::from_utf8(output.stdout)
        .context("git output not UTF-8")?
        .trim()
        .to_string();
    Ok(stdout)
}

/// Get the current branch name of a worktree.
pub async fn current_branch(wt_path: &Path) -> Result<String> {
    run_git(wt_path, &["rev-parse", "--abbrev-ref", "HEAD"]).await
}

/// Get the HEAD SHA of a worktree (short form).
pub(crate) async fn head_sha(cwd: &Path) -> Result<String> {
    run_git(cwd, &["rev-parse", "--short", "HEAD"]).await
}
