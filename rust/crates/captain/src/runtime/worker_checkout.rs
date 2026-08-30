//! Worktree + branch resolution shared by every worker spawn adapter.
//!
//! Claude, Codex and OpenCode all reach their agent with the same checkout:
//! fetch origin, decide a plan from the task's stored binding, then reuse,
//! reset or recreate the worktree. Only the tracing module label differs,
//! so each adapter passes its own.

use std::path::{Path, PathBuf};

use anyhow::Result;
use settings::ProjectConfig;

use super::task_creation::next_worker_slot;
use crate::Task;

/// Decision for how a worker spawn should resolve a task's worktree + branch.
///
/// Captain invariant #4 (see CLAUDE.md): once a task has a worktree
/// assigned, that path is permanent for the task's lifetime. After
/// PR #991 the workbench + worktree are created atomically with the
/// task itself, so spawn never allocates a fresh slot — it only resumes
/// (`Reuse`), reworks the existing tree (`Rework`), or recreates the
/// worktree at its stored path when the directory disappears
/// (`Recreate`).
#[derive(Debug)]
pub(crate) enum WorktreePlan {
    /// Reopen — worktree exists on disk, branch is known. Reuse as-is.
    Reuse { wt: PathBuf, branch: String },
    /// Rework — worktree exists on disk, branch is cleared. Same
    /// worktree, fresh branch from origin/main.
    Rework { wt: PathBuf },
    /// Worktree binding is set but the directory was removed out from
    /// under the task. Recreate `git worktree` at the stored path with a
    /// fresh branch; do NOT allocate a new slot.
    Recreate {
        wt: PathBuf,
        stored_branch: Option<String>,
    },
    /// Impossible state — task reached spawn with no stored worktree.
    /// Eager workbench+worktree creation removed the legitimate "fresh"
    /// path; this variant exists so spawn can refuse cleanly instead of
    /// silently allocating.
    MissingBinding,
}

/// Pure decision: which worktree plan should a worker spawn execute?
///
/// Separated so the branch selection is unit-testable without a real git
/// repo. All IO (next_worker_slot, git commands, logging) happens in
/// `resolve_worker_checkout` based on the returned variant.
pub(crate) fn plan_worktree(stored_wt: Option<&PathBuf>, branch: Option<&str>) -> WorktreePlan {
    match (stored_wt, branch) {
        (Some(wt), Some(b)) if wt.exists() => WorktreePlan::Reuse {
            wt: wt.clone(),
            branch: b.to_string(),
        },
        (Some(wt), None) if wt.exists() => WorktreePlan::Rework { wt: wt.clone() },
        (Some(wt), stored) => WorktreePlan::Recreate {
            wt: wt.clone(),
            stored_branch: stored.map(str::to_string),
        },
        (None, _) => WorktreePlan::MissingBinding,
    }
}

pub(crate) fn new_slug(item: &Task, slot: u64) -> String {
    format!("todo-{}-{}", item.id, slot)
}

/// Fetch origin, then execute the task's worktree plan and return the
/// branch the worker will build on plus the worktree it runs in.
#[tracing::instrument(skip(project_config, item), fields(task_id = item.id, adapter))]
pub(crate) async fn resolve_worker_checkout(
    project_config: &ProjectConfig,
    item: &Task,
    adapter: &'static str,
) -> Result<(String, PathBuf)> {
    let repo_path = global_infra::paths::expand_tilde(&project_config.path);
    // Fetch origin so we branch off the latest remote HEAD.
    global_git::fetch_origin(&repo_path).await?;
    let stored_wt = item
        .worktree
        .as_deref()
        .map(global_infra::paths::expand_tilde);
    let default_branch = global_git::default_branch(&repo_path).await?;

    match plan_worktree(stored_wt.as_ref(), item.branch.as_deref()) {
        WorktreePlan::Reuse { wt, branch } => {
            tracing::info!(
                module = adapter,
                worktree = %wt.display(),
                branch = %branch,
                "reusing existing worktree for reopened item"
            );
            Ok((branch, wt))
        }
        WorktreePlan::Rework { wt } => {
            // Rework: same worktree, fresh branch from origin/main.
            let slug = new_slug(item, next_worker_slot(&global_infra::paths::state_dir())?);
            let branch = format!("mando/{}", slug);
            tracing::info!(
                module = adapter,
                worktree = %wt.display(),
                branch = %branch,
                "rework: resetting existing worktree to new branch"
            );
            global_git::reset_to_new_branch(&wt, &branch, &default_branch).await?;
            Ok((branch, wt))
        }
        WorktreePlan::Recreate { wt, stored_branch } => {
            // Worktree binding set, directory missing on disk. Captain
            // invariant #4 keeps the assigned worktree permanent, so we
            // recreate it at the stored path with a fresh branch. Eager
            // workbench creation made the workbench-per-task mapping
            // permanent: spawn never mints workbench rows, so the only
            // safe response to a missing directory is to rebuild it at
            // the stored path.
            tracing::warn!(
                module = adapter,
                task_id = item.id,
                worktree = %wt.display(),
                stored_branch = ?stored_branch,
                "task worktree dir missing on disk — recreating at stored path to preserve invariant #4 (worktree permanent)"
            );
            let branch =
                recreate_worktree_at(item, &repo_path, &default_branch, &wt, adapter).await?;
            Ok((branch, wt))
        }
        WorktreePlan::MissingBinding => {
            // After eager workbench+worktree creation, every task reaches
            // spawn with a stored worktree binding. Reaching this branch
            // means upstream code dropped the binding (or a tick saw the
            // task pre-creation, which the lifecycle now disallows).
            // Refuse to spawn rather than silently reallocating.
            anyhow::bail!(
                "task {} reached spawn without a worktree binding -- refusing to spawn (impossible state after eager workbench creation)",
                item.id,
            );
        }
    }
}

/// Recreate a missing worktree at its stored path with a fresh slot-derived
/// branch. Retries on `WorktreeAlreadyExists` (leftover metadata, counter
/// reuse, concurrent slot allocation) by rotating the branch slug; the path
/// itself stays pinned to `wt` so captain invariant #4 is preserved.
async fn recreate_worktree_at(
    item: &Task,
    repo_path: &Path,
    default_branch: &str,
    wt: &Path,
    adapter: &'static str,
) -> Result<String> {
    const MAX_ATTEMPTS: usize = 20;

    for attempt in 0..MAX_ATTEMPTS {
        let slug = new_slug(item, next_worker_slot(&global_infra::paths::state_dir())?);
        let branch = format!("mando/{}", slug);

        if let Err(e) = global_git::prune_worktrees(repo_path).await {
            tracing::warn!(module = adapter, error = %e, "failed to prune stale git worktree metadata before recreate");
        }
        if let Err(e) = global_git::delete_local_branch(repo_path, &branch).await {
            tracing::debug!(module = adapter, branch = %branch, error = %e, "stale branch cleanup before recreate (expected if branch doesn't exist)");
        }

        match global_git::create_worktree(repo_path, &branch, wt, default_branch).await {
            Ok(()) => {
                crate::io::worktree_bootstrap::copy_local_files(repo_path, wt).await;
                return Ok(branch);
            }
            Err(e)
                if crate::find_git_error(&e).is_some_and(|g| {
                    matches!(g, crate::GitError::WorktreeAlreadyExists { .. })
                }) && attempt + 1 < MAX_ATTEMPTS =>
            {
                tracing::warn!(
                    module = adapter,
                    branch = %branch,
                    worktree = %wt.display(),
                    attempt = attempt + 1,
                    "branch/worktree already exists during recreate — rotating slug and retrying"
                );
            }
            Err(e) => return Err(e),
        }
    }

    anyhow::bail!(
        "failed to recreate worktree at {} after {} attempts for task {}",
        wt.display(),
        MAX_ATTEMPTS,
        item.id
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_worktree() -> std::path::PathBuf {
        let path =
            std::env::temp_dir().join(format!("mando-spawner-{}", global_infra::uuid::Uuid::v4()));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    // ── plan_worktree: branch-selection decision (captain invariant #4) ──

    #[test]
    fn plan_reuse_when_wt_exists_and_branch_present() {
        let wt = temp_worktree();
        let plan = plan_worktree(Some(&wt), Some("mando/todo-7-3"));
        match plan {
            WorktreePlan::Reuse { wt: got, branch } => {
                assert_eq!(got, wt);
                assert_eq!(branch, "mando/todo-7-3");
            }
            other => panic!("expected Reuse, got {other:?}"),
        }
    }

    #[test]
    fn plan_rework_when_wt_exists_and_branch_cleared() {
        let wt = temp_worktree();
        let plan = plan_worktree(Some(&wt), None);
        match plan {
            WorktreePlan::Rework { wt: got } => assert_eq!(got, wt),
            other => panic!("expected Rework, got {other:?}"),
        }
    }

    #[test]
    fn plan_recreate_when_wt_missing_on_disk_with_branch() {
        // Captain invariant #4 — the stored worktree binding is permanent.
        // A missing directory must route to Recreate (recreate at the
        // stored path), never to MissingBinding. After eager workbench
        // creation, the workbench's worktree path is permanent; spawn
        // recovers the directory rather than allocating a new slot or
        // minting a second workbench row.
        let wt: PathBuf =
            std::env::temp_dir().join(format!("mando-missing-{}", global_infra::uuid::Uuid::v4()));
        assert!(!wt.exists(), "sanity: missing path must not exist");
        let plan = plan_worktree(Some(&wt), Some("mando/todo-7-3"));
        match plan {
            WorktreePlan::Recreate {
                wt: got,
                stored_branch,
            } => {
                assert_eq!(got, wt, "recreate must target the ORIGINAL stored path");
                assert_eq!(stored_branch.as_deref(), Some("mando/todo-7-3"));
            }
            other => panic!("expected Recreate, got {other:?}"),
        }
    }

    #[test]
    fn plan_recreate_when_wt_missing_on_disk_and_branch_cleared() {
        // Same invariant applies after a respawn verdict — branch is None
        // but the worktree binding must still survive a missing dir.
        let wt: PathBuf =
            std::env::temp_dir().join(format!("mando-missing-{}", global_infra::uuid::Uuid::v4()));
        assert!(!wt.exists());
        let plan = plan_worktree(Some(&wt), None);
        match plan {
            WorktreePlan::Recreate {
                wt: got,
                stored_branch,
            } => {
                assert_eq!(got, wt);
                assert!(stored_branch.is_none());
            }
            other => panic!("expected Recreate, got {other:?}"),
        }
    }

    #[test]
    fn plan_missing_binding_when_no_worktree_stored() {
        // Eager workbench+worktree creation makes the no-worktree case
        // impossible during spawn. plan_worktree surfaces it as a typed
        // variant so the spawn caller can refuse cleanly.
        let plan = plan_worktree(None, None);
        match plan {
            WorktreePlan::MissingBinding => {}
            other => panic!("expected MissingBinding, got {other:?}"),
        }
    }

    #[test]
    fn plan_missing_binding_when_branch_set_without_worktree() {
        // A branch without a worktree binding is the same impossible
        // state — the eager-creation lifecycle never produces it, so
        // spawn rejects it.
        let plan = plan_worktree(None, Some("mando/orphan"));
        match plan {
            WorktreePlan::MissingBinding => {}
            other => panic!("expected MissingBinding, got {other:?}"),
        }
    }
}
