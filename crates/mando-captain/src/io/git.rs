//! Git worktree, branch, and rebase operations.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Create a git worktree with a new branch.
pub async fn create_worktree(
    repo_path: &Path,
    branch: &str,
    wt_path: &Path,
    base_ref: &str,
) -> Result<()> {
    let output = tokio::process::Command::new("git")
        .args(["worktree", "add", "-b", branch])
        .arg(wt_path)
        .arg(base_ref)
        .current_dir(repo_path)
        .output()
        .await
        .context("git worktree add")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git worktree add failed: {}", stderr);
    }
    Ok(())
}

/// Remove a git worktree. Falls back to rm -rf if git remove fails.
/// Returns error only if both methods fail.
pub async fn remove_worktree(repo_path: &Path, wt_path: &Path) -> Result<()> {
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
            .with_context(|| format!("rm -rf also failed for {}", wt_path.display()))?;
    }
    Ok(())
}

/// Delete a local branch.
pub async fn delete_local_branch(repo_path: &Path, branch: &str) -> Result<()> {
    let output = tokio::process::Command::new("git")
        .args(["branch", "-D", branch])
        .current_dir(repo_path)
        .output()
        .await
        .context("git branch -D")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git branch -D {branch} failed: {}", stderr.trim());
    }
    Ok(())
}

/// Fetch from origin so local refs are up to date.
pub async fn fetch_origin(repo_path: &Path) -> Result<()> {
    let output = tokio::process::Command::new("git")
        .args(["fetch", "origin"])
        .current_dir(repo_path)
        .output()
        .await
        .context("git fetch origin")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("git fetch origin failed: {}", stderr.trim());
    }
    Ok(())
}

/// Get the default branch name (main or master).
pub async fn default_branch(repo_path: &Path) -> Result<String> {
    let output = tokio::process::Command::new("git")
        .args(["symbolic-ref", "refs/remotes/origin/HEAD", "--short"])
        .current_dir(repo_path)
        .output()
        .await?;

    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() {
        tracing::debug!(
            module = "git",
            path = %repo_path.display(),
            "symbolic-ref returned empty, falling back to origin/main"
        );
        Ok("origin/main".to_string())
    } else {
        Ok(text)
    }
}

/// List all worktree paths for a repo.
pub async fn list_worktrees(repo_path: &Path) -> Result<Vec<PathBuf>> {
    let output = tokio::process::Command::new("git")
        .args(["worktree", "list", "--porcelain"])
        .current_dir(repo_path)
        .output()
        .await
        .context("git worktree list")?;

    let text = String::from_utf8_lossy(&output.stdout);
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
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "git {} failed: {}",
            args.first().unwrap_or(&"?"),
            stderr.trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Get the HEAD SHA of a worktree (short form).
pub(crate) async fn head_sha(cwd: &Path) -> Result<String> {
    run_git(cwd, &["rev-parse", "--short", "HEAD"]).await
}
