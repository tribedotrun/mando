//! Local git CLI provider boundary.
//!
//! This crate is the only production code allowed to spawn `git`. Callers own
//! orchestration policy; this crate owns command execution, stdout/stderr
//! parsing, and typed git failure variants.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Failure modes from git helpers that callers branch on.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum GitError {
    /// `git worktree add -b <branch>` saw the branch or directory already
    /// exists — caller typically retries in a fresh slot.
    #[error("git worktree/branch {branch:?} already exists")]
    WorktreeAlreadyExists { branch: String },
    /// `git commit` refused because nothing was staged — caller treats as
    /// a no-op rather than a hard failure.
    #[error("nothing to commit")]
    NothingToCommit,
}

pub fn find_git_error(err: &anyhow::Error) -> Option<&GitError> {
    err.chain().find_map(|src| src.downcast_ref::<GitError>())
}

/// Create a git worktree with a new branch.
pub async fn create_worktree(
    repo_path: &Path,
    branch: &str,
    wt_path: &Path,
    base_ref: &str,
) -> Result<()> {
    let wt_str = wt_path
        .to_str()
        .context("worktree path is not valid UTF-8")?;
    let args = &["worktree", "add", "-b", branch, wt_str, base_ref];
    let output = tokio::process::Command::new("git")
        .args(args)
        .current_dir(repo_path)
        .output()
        .await
        .context("git worktree add")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let trimmed = stderr.trim();
        if trimmed.contains("already exists") {
            return Err(GitError::WorktreeAlreadyExists {
                branch: branch.to_string(),
            }
            .into());
        }
        anyhow::bail!("git worktree add failed: {trimmed}");
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
            .with_context(|| format!("rm -rf also failed for {wt_str}"))?;
    }
    Ok(())
}

pub async fn delete_local_branch(repo_path: &Path, branch: &str) -> Result<()> {
    run_git(repo_path, &["branch", "-D", branch]).await?;
    Ok(())
}

pub async fn delete_remote_branch(repo_path: &Path, branch: &str) -> Result<()> {
    run_git(repo_path, &["push", "origin", "--delete", branch]).await?;
    Ok(())
}

pub async fn prune_worktrees(repo_path: &Path) -> Result<()> {
    run_git(repo_path, &["worktree", "prune"]).await?;
    Ok(())
}

pub async fn fetch_origin(repo_path: &Path) -> Result<()> {
    run_git(repo_path, &["fetch", "origin"]).await?;
    Ok(())
}

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
    global_types::data_dir().join("worktrees")
}

/// Compute worktree path: `<worktrees_dir>/<repo_name>-<slug>`.
pub fn worktree_path(repo_path: &Path, slug: &str) -> PathBuf {
    let repo_name = repo_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("repo");
    worktrees_dir().join(format!("{repo_name}-{slug}"))
}

pub async fn reset_to_new_branch(wt_path: &Path, branch: &str, base_ref: &str) -> Result<()> {
    run_git(wt_path, &["reset", "--hard"]).await?;
    run_git(wt_path, &["clean", "-fd"]).await?;
    run_git(wt_path, &["checkout", "-B", branch, base_ref]).await?;
    Ok(())
}

pub async fn current_branch(wt_path: &Path) -> Result<String> {
    run_git(wt_path, &["rev-parse", "--abbrev-ref", "HEAD"]).await
}

pub async fn branch_show_current(wt_path: &Path) -> Result<String> {
    run_git(wt_path, &["branch", "--show-current"]).await
}

pub async fn checked_out_branch(wt_path: &Path) -> Result<String> {
    match branch_show_current(wt_path).await {
        Ok(branch) if !branch.is_empty() && branch != "HEAD" => Ok(branch),
        _ => current_branch(wt_path).await,
    }
}

/// Get the full HEAD SHA of a worktree.
pub async fn head_sha(cwd: &Path) -> Result<String> {
    run_git(cwd, &["rev-parse", "HEAD"]).await
}

/// Get the short HEAD SHA of a worktree.
pub async fn head_sha_short(cwd: &Path) -> Result<String> {
    run_git(cwd, &["rev-parse", "--short", "HEAD"]).await
}

/// Return the main repository path for a linked worktree.
pub async fn common_repo_path(wt_path: &Path) -> Result<Option<PathBuf>> {
    let git_dir = run_git(wt_path, &["rev-parse", "--git-common-dir"]).await?;
    Ok(Path::new(&git_dir).parent().map(Path::to_path_buf))
}

/// Abort an in-progress rebase. Returns true when git reported success.
pub async fn abort_rebase(wt_path: &Path) -> Result<bool> {
    let status = tokio::process::Command::new("git")
        .args(["rebase", "--abort"])
        .current_dir(wt_path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .await
        .context("git rebase --abort")?;
    Ok(status.success())
}

pub async fn add_force(cwd: &Path, rel_path: &str) -> Result<()> {
    run_git(cwd, &["add", "-f", rel_path]).await?;
    Ok(())
}

pub async fn commit_allow_empty(cwd: &Path, message: &str) -> Result<()> {
    let output = tokio::process::Command::new("git")
        .args(["commit", "--allow-empty", "-m", message])
        .current_dir(cwd)
        .output()
        .await
        .context("git commit")?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let trimmed = stderr.trim();
    if trimmed.contains("nothing to commit") {
        return Err(GitError::NothingToCommit.into());
    }
    anyhow::bail!("git commit failed: {trimmed}");
}

pub async fn push_set_upstream_force(cwd: &Path, remote: &str, branch: &str) -> Result<()> {
    run_git(cwd, &["push", "-u", "--force", remote, branch]).await?;
    Ok(())
}

pub async fn is_repository(path: &Path) -> Result<bool> {
    if tokio::fs::try_exists(path.join(".git"))
        .await
        .unwrap_or(false)
    {
        return Ok(true);
    }

    let output = tokio::process::Command::new("git")
        .args(["rev-parse", "--git-dir"])
        .current_dir(path)
        .output()
        .await
        .context("git rev-parse --git-dir")?;
    Ok(output.status.success())
}

pub async fn detect_github_repo(path: &str) -> Option<String> {
    let abs = global_infra::paths::expand_tilde(path);
    let output = tokio::process::Command::new("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(&abs)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .output()
        .await
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
    parse_github_slug(&url)
}

pub fn parse_github_slug(url: &str) -> Option<String> {
    let url = url.trim();
    if let Some(rest) = url.strip_prefix("git@github.com:") {
        let slug = rest.trim_end_matches(".git");
        if slug.contains('/') {
            return Some(slug.to_string());
        }
    }
    if let Some(idx) = url.find("github.com/") {
        let slug = url[idx + "github.com/".len()..].trim_end_matches(".git");
        if slug.contains('/') && !slug.contains(' ') {
            return Some(slug.to_string());
        }
    }
    None
}

/// Run a git command and return trimmed stdout. Fails on non-zero exit.
async fn run_git(cwd: &Path, args: &[&str]) -> Result<String> {
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
    let stdout = String::from_utf8(output.stdout)
        .context("git output not UTF-8")?
        .trim()
        .to_string();
    Ok(stdout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_includes_branch() {
        let err = GitError::WorktreeAlreadyExists {
            branch: "mando/foo-1".into(),
        };
        assert!(err.to_string().contains("mando/foo-1"));
    }

    #[test]
    fn find_survives_context_wrapping() {
        let wrapped = anyhow::Error::new(GitError::NothingToCommit).context("scaffold commit");
        let typed = find_git_error(&wrapped).expect("typed error reachable");
        assert!(matches!(typed, GitError::NothingToCommit));
    }

    #[test]
    fn parse_github_slug_formats() {
        assert_eq!(
            parse_github_slug("git@github.com:acme/widgets.git"),
            Some("acme/widgets".to_string())
        );
        assert_eq!(
            parse_github_slug("https://github.com/acme/widgets.git"),
            Some("acme/widgets".to_string())
        );
        assert_eq!(parse_github_slug("git@gitlab.com:org/repo.git"), None);
    }
}
