//! Task cleanup. Removes worktree, branches (local + remote), health entry,
//! workbench, CC stream files, plans, and closes the PR when deleting tasks.

use crate::Task;
use anyhow::Result;
use settings::Config;

/// Options controlling what external resources to clean up alongside the task.
#[derive(Debug, Clone, Default)]
pub struct CleanupOptions {
    pub close_pr: bool,
    /// Skip the active-worker guard and force-delete regardless of task status.
    pub force: bool,
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
        settings::resolve_project_config(Some(&item.project), config)
            .map(|(_, pc)| global_infra::paths::expand_tilde(&pc.path))
    };

    {
        let cc_sid = item.session_ids.worker.as_deref().unwrap_or("");
        let pid = super::pid_registry::get_verified_pid(cc_sid).unwrap_or(crate::Pid::new(0));
        if pid.as_u32() > 0 {
            if let Err(e) = global_claude::kill_process(pid).await {
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
        let wt_path = global_infra::paths::expand_tilde(wt);
        if wt_path.exists() {
            global_git::current_branch(&wt_path).await.ok()
        } else {
            None
        }
    } else {
        None
    };
    let branch_to_delete = live_branch.as_deref().or(item.branch.as_deref());

    if let Some(ref wt) = item.worktree {
        let wt_path = global_infra::paths::expand_tilde(wt);
        if wt_path.exists() {
            if let Some(ref rp) = repo_path {
                match global_git::remove_worktree(rp, &wt_path).await {
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
            if let Err(e) = global_git::delete_local_branch(rp, branch).await {
                let msg = format!("delete_local_branch {branch}: {e}");
                tracing::warn!(module = "cleanup", branch = %branch, error = %e, "failed to delete branch");
                warnings.push(msg);
            } else {
                tracing::info!(module = "cleanup", branch = %branch, "deleted branch");
            }

            // Also remove the remote branch so it doesn't linger on GitHub.
            if let Err(e) = global_git::delete_remote_branch(rp, branch).await {
                let msg = format!("delete_remote_branch {branch}: {e}");
                tracing::warn!(module = "cleanup", branch = %branch, error = %e, "failed to delete remote branch");
                warnings.push(msg);
            } else {
                tracing::info!(module = "cleanup", branch = %branch, "deleted remote branch");
            }
        }
    }

    if let Some(ref worker) = item.worker {
        let health_path = crate::config::worker_health_path();
        match super::health_store::load_health_state(&health_path) {
            Ok(mut health) => {
                health.remove(worker.as_str());
                if let Err(e) = super::health_store::save_health_state(&health_path, &health) {
                    let msg = format!("save_health_state {}: {e}", health_path.display());
                    tracing::warn!(module = "captain-io-task_cleanup", path = %health_path.display(), error = %e, "failed to save health state during cleanup");
                    warnings.push(msg);
                }
            }
            Err(e) => {
                let msg = format!("load_health_state {}: {e}", health_path.display());
                tracing::warn!(module = "captain-io-task_cleanup", path = %health_path.display(), error = %e, "failed to load health state during cleanup");
                warnings.push(msg);
            }
        }
    }

    {
        let id_str = item.id.to_string();
        let lock_dir = global_infra::paths::state_dir().join("item-locks");
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
            super::timeline_store::timeline_path(&global_infra::paths::state_dir(), &id_str);
        if let Err(e) = tokio::fs::remove_file(&timeline_path).await {
            if e.kind() != std::io::ErrorKind::NotFound {
                let msg = format!("remove timeline {}: {e}", timeline_path.display());
                tracing::warn!(module = "cleanup", path = %timeline_path.display(), error = %e, "failed to remove timeline file");
                warnings.push(msg);
            }
        }
    }

    // Collect session IDs before deleting DB rows so we can clean up stream
    // files on disk. The stream files (jsonl, meta.json, stderr) are not
    // managed by the DB and would otherwise be orphaned.
    let session_ids: Vec<String> = sessions_db::list_sessions_for_task(pool, item.id)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|s| s.session_id)
        .collect();

    {
        match sessions_db::delete_sessions_for_task(pool, item.id).await {
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

    // Remove CC stream files (.jsonl, .meta.json, .stderr) for each session.
    for sid in &session_ids {
        for ext in &["jsonl", "meta.json", "stderr"] {
            let path = global_infra::paths::cc_streams_dir().join(format!("{sid}.{ext}"));
            global_infra::best_effort!(
                tokio::fs::remove_file(&path).await,
                "task_cleanup: tokio::fs::remove_file(&path).await"
            );
        }
    }
    if !session_ids.is_empty() {
        tracing::info!(module = "cleanup", item_id = %item.id, count = session_ids.len(), "removed CC stream files");
    }

    // Remove the plans/brief file for this task.
    {
        let plans_dir = global_infra::paths::state_dir().join("plans");
        let brief_path = plans_dir.join(format!("item-{}.md", item.id));
        if let Err(e) = tokio::fs::remove_file(&brief_path).await {
            if e.kind() != std::io::ErrorKind::NotFound {
                let msg = format!("remove plan file {}: {e}", brief_path.display());
                tracing::warn!(module = "cleanup", path = %brief_path.display(), error = %e, "failed to remove plan file");
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
                    settings::resolve_github_repo(Some(&item.project), config)
                }
            });
            match repo {
                Some(repo) => match global_github::close_pr(&repo, &pr_num_str).await {
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

    // Soft-delete the workbench so it disappears from the sidebar.
    if item.workbench_id > 0 {
        let wb_id = item.workbench_id;
        if let Err(e) = crate::io::queries::workbenches::mark_deleted(pool, wb_id).await {
            let msg = format!("mark_deleted workbench {wb_id}: {e}");
            tracing::warn!(module = "cleanup", workbench_id = wb_id, error = %e, "failed to soft-delete workbench");
            warnings.push(msg);
        } else {
            tracing::info!(
                module = "cleanup",
                workbench_id = wb_id,
                "soft-deleted workbench"
            );
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
