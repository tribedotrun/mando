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
#[tracing::instrument(skip_all)]
pub(crate) async fn sync_branches(items: &mut [Task]) {
    for item in items.iter_mut() {
        let Some(worktree) = item.worktree.as_deref() else {
            continue;
        };
        let wt_path = global_infra::paths::expand_tilde(worktree);
        if !wt_path.exists() {
            // Captain invariant #4: worktree is permanent. The spawner's
            // Recreate arm recovers from this, but we must (a) surface
            // the disk drift with a loud warn and (b) clear the stale
            // `item.branch` so the resume/rework branch decision never
            // operates on a branch name that matches a deleted worktree.
            tracing::warn!(
                module = "captain",
                task_id = item.id,
                path = %wt_path.display(),
                stored_branch = ?item.branch,
                "task worktree missing on disk during branch sync — clearing stale branch so next spawn enters Recreate cleanly"
            );
            item.branch = None;
            continue;
        }
        if let Ok(live) = global_git::current_branch(&wt_path).await {
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
