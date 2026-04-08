//! Task cleanup. Removes worktree, branch, health entry, close PR when deleting tasks.

use anyhow::Result;
use mando_config::settings::Config;
use mando_types::Task;

/// Options controlling what external resources to clean up alongside the task.
#[derive(Debug, Clone, Default)]
pub struct CleanupOptions {
    pub close_pr: bool,
}

pub(crate) async fn cleanup_task(
    item: &Task,
    config: &Config,
    pool: &sqlx::SqlitePool,
    opts: &CleanupOptions,
) -> Result<Vec<String>> {
    let mut warnings: Vec<String> = Vec::new();

    let repo_path = if item.project.is_empty() {
        None
    } else {
        mando_config::resolve_project_config(Some(&item.project), config)
            .map(|(_, pc)| mando_config::expand_tilde(&pc.path))
    };

    {
        let cc_sid = item.session_ids.worker.as_deref().unwrap_or("");
        let pid = super::pid_registry::get_pid(cc_sid).unwrap_or(mando_types::Pid::new(0));
        if pid.as_u32() > 0 {
            if let Err(e) = mando_cc::kill_process(pid).await {
                let msg = format!("kill worker pid {pid}: {e}");
                tracing::warn!(module = "cleanup", worker = ?item.worker, pid = %pid, error = %e, "failed to kill worker process");
                warnings.push(msg);
            } else {
                tracing::info!(module = "cleanup", worker = ?item.worker, pid = %pid, "killed worker");
            }
        }
        if !cc_sid.is_empty() {
            if let Err(e) = super::pid_registry::unregister(cc_sid) {
                let msg = format!("pid_registry unregister {cc_sid}: {e}");
                tracing::warn!(module = "cleanup", cc_sid = %cc_sid, error = %e, "failed to unregister pid");
                warnings.push(msg);
            }
        }
    }

    // Read the live branch from the worktree before removing it — the DB
    // value can be stale if mando-pr renamed the branch.
    let live_branch = if let Some(ref wt) = item.worktree {
        let wt_path = mando_config::expand_tilde(wt);
        if wt_path.exists() {
            super::git::current_branch(&wt_path).await.ok()
        } else {
            None
        }
    } else {
        None
    };
    let branch_to_delete = live_branch.as_deref().or(item.branch.as_deref());

    if let Some(ref wt) = item.worktree {
        let wt_path = mando_config::expand_tilde(wt);
        if wt_path.exists() {
            if let Some(ref rp) = repo_path {
                match super::git::remove_worktree(rp, &wt_path).await {
                    Ok(_) => {
                        tracing::info!(module = "cleanup", path = %wt_path.display(), "removed worktree")
                    }
                    Err(e) => {
                        let msg = format!("remove_worktree {}: {e}", wt_path.display());
                        tracing::warn!(module = "cleanup", error = %e, "failed to remove worktree");
                        warnings.push(msg);
                    }
                }
            } else {
                match tokio::fs::remove_dir_all(&wt_path).await {
                    Ok(_) => {
                        tracing::info!(module = "cleanup", path = %wt_path.display(), "removed worktree dir (no repo context)")
                    }
                    Err(e) => {
                        let msg = format!("remove_dir_all {}: {e}", wt_path.display());
                        tracing::warn!(module = "cleanup", path = %wt_path.display(), error = %e, "failed to remove worktree dir");
                        warnings.push(msg);
                    }
                }
            }
        }
    }

    if let Some(branch) = branch_to_delete {
        if let Some(ref rp) = repo_path {
            if let Err(e) = super::git::delete_local_branch(rp, branch).await {
                let msg = format!("delete_local_branch {branch}: {e}");
                tracing::warn!(module = "cleanup", branch = %branch, error = %e, "failed to delete branch");
                warnings.push(msg);
            } else {
                tracing::info!(module = "cleanup", branch = %branch, "deleted branch");
            }
        }
    }

    if let Some(ref worker) = item.worker {
        let health_path = mando_config::worker_health_path();
        match super::health_store::load_health_state(&health_path) {
            Ok(mut health) => {
                health.remove(worker.as_str());
                if let Err(e) = super::health_store::save_health_state(&health_path, &health) {
                    let msg = format!("save_health_state {}: {e}", health_path.display());
                    tracing::warn!(path = %health_path.display(), error = %e, "failed to save health state during cleanup");
                    warnings.push(msg);
                }
            }
            Err(e) => {
                let msg = format!("load_health_state {}: {e}", health_path.display());
                tracing::warn!(path = %health_path.display(), error = %e, "failed to load health state during cleanup");
                warnings.push(msg);
            }
        }
    }

    {
        let id_str = item.id.to_string();
        let lock_dir = mando_config::state_dir().join("item-locks");
        let lock_path = lock_dir.join(format!("{id_str}.lock"));
        if let Err(e) = tokio::fs::remove_file(&lock_path).await {
            if e.kind() != std::io::ErrorKind::NotFound {
                let msg = format!("remove lock file {}: {e}", lock_path.display());
                tracing::warn!(module = "cleanup", path = %lock_path.display(), error = %e, "failed to remove item lock file");
                warnings.push(msg);
            }
        }
    }

    {
        let id_str = item.id.to_string();
        let timeline_path =
            super::timeline_store::timeline_path(&mando_config::state_dir(), &id_str);
        if let Err(e) = tokio::fs::remove_file(&timeline_path).await {
            if e.kind() != std::io::ErrorKind::NotFound {
                let msg = format!("remove timeline {}: {e}", timeline_path.display());
                tracing::warn!(module = "cleanup", path = %timeline_path.display(), error = %e, "failed to remove timeline file");
                warnings.push(msg);
            }
        }
    }

    {
        match mando_db::queries::sessions::delete_sessions_for_task(pool, item.id).await {
            Ok(n) => {
                if n > 0 {
                    tracing::info!(module = "cleanup", item_id = %item.id, deleted = n, "purged session entries");
                }
            }
            Err(e) => {
                let msg = format!("delete_sessions_for_task {}: {e}", item.id);
                tracing::warn!(module = "cleanup", task_id = %item.id, error = %e, "failed to delete sessions");
                warnings.push(msg);
            }
        }
    }

    // Close GitHub PR if requested
    if opts.close_pr {
        if let Some(pr_num) = item.pr_number {
            let pr_num_str = pr_num.to_string();
            // Prefer stored github_repo (populated via JOIN), fall back to config
            let repo = item.github_repo.clone().or_else(|| {
                if item.project.is_empty() {
                    None
                } else {
                    mando_config::resolve_github_repo(Some(&item.project), config)
                }
            });
            match repo {
                Some(repo) => match super::github::close_pr(&repo, &pr_num_str).await {
                    Ok(()) => {
                        tracing::info!(module = "cleanup", pr_number = pr_num, repo = %repo, "closed PR");
                    }
                    Err(e) => {
                        let msg = format!("Failed to close PR #{pr_num}: {e}");
                        tracing::warn!(module = "cleanup", pr_number = pr_num, error = %e, "failed to close PR");
                        warnings.push(msg);
                    }
                },
                None => {
                    let msg =
                        format!("Cannot close PR #{pr_num}: no github_repo configured for project");
                    tracing::warn!(module = "cleanup", pr_number = pr_num, "{}", msg);
                    warnings.push(msg);
                }
            }
        }
    }

    Ok(warnings)
}

pub(crate) async fn cleanup_tasks(
    items: &[Task],
    config: &Config,
    pool: &sqlx::SqlitePool,
    opts: &CleanupOptions,
) -> Vec<String> {
    let mut all_warnings = Vec::new();
    for item in items {
        match cleanup_task(item, config, pool, opts).await {
            Ok(w) => all_warnings.extend(w),
            Err(e) => {
                tracing::warn!(module = "cleanup", title = %item.title, error = %e, "error cleaning up");
            }
        }
    }
    all_warnings
}
