//! Sync branch names from worktrees into task items.
//!
//! The mando-pr skill renames branches for better PR titles, making the
//! DB value stale. This module reads the live branch from each worktree
//! and updates items so all downstream phases use the current name.

use crate::Task;

/// Sync branch names from worktrees for all items with a worktree.
///
/// Runs unconditionally (even for items with PRs) so that `item.branch`
/// is always populated after a daemon restart (the column is no longer
/// in the DB). Skips detached HEAD state.
pub(crate) async fn sync_branches(items: &mut [Task]) {
    for item in items.iter_mut() {
        if item.worktree.is_none() {
            continue;
        }
        let wt_path = global_infra::paths::expand_tilde(item.worktree.as_deref().unwrap());
        if !wt_path.exists() {
            continue;
        }
        if let Ok(live) = crate::io::git::current_branch(&wt_path).await {
            if live == "HEAD" {
                continue; // detached HEAD
            }
            if item.branch.as_deref() != Some(live.as_str()) {
                tracing::info!(
                    module = "captain",
                    title = %item.title,
                    old = ?item.branch,
                    new = %live,
                    "synced stale branch from worktree"
                );
                item.branch = Some(live);
            }
        }
    }
}
