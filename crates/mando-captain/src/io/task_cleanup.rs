//! Task cleanup — remove worktree, branch, health entry, close PR, cancel Linear issue
//! when deleting tasks.

use anyhow::Result;
use mando_config::settings::Config;
use mando_types::Task;

/// Options controlling what external resources to clean up alongside the task.
#[derive(Debug, Clone, Default)]
pub struct CleanupOptions {
    pub close_pr: bool,
    pub cancel_linear: bool,
}

pub(crate) async fn cleanup_task(
    item: &Task,
    config: &Config,
    pool: &sqlx::SqlitePool,
    opts: &CleanupOptions,
) -> Result<Vec<String>> {
    let mut warnings: Vec<String> = Vec::new();

    let repo_path = item.project.as_deref().and_then(|name| {
        mando_config::resolve_project_config(Some(name), config)
            .map(|(_, pc)| mando_config::expand_tilde(&pc.path))
    });

    {
        let cc_sid = item.session_ids.worker.as_deref().unwrap_or("");
        let pid = super::pid_registry::get_pid(cc_sid).unwrap_or(0);
        if pid > 0 {
            if let Err(e) = mando_cc::kill_process(pid).await {
                tracing::warn!(module = "cleanup", worker = ?item.worker, pid = pid, error = %e, "failed to kill worker process");
            } else {
                tracing::info!(module = "cleanup", worker = ?item.worker, pid = pid, "killed worker");
            }
        }
        if !cc_sid.is_empty() {
            super::pid_registry::unregister(cc_sid);
        }
    }

    if let Some(ref wt) = item.worktree {
        let wt_path = mando_config::expand_tilde(wt);
        if wt_path.exists() {
            if let Some(ref rp) = repo_path {
                match super::git::remove_worktree(rp, &wt_path).await {
                    Ok(_) => {
                        tracing::info!(module = "cleanup", path = %wt_path.display(), "removed worktree")
                    }
                    Err(e) => {
                        tracing::warn!(module = "cleanup", error = %e, "failed to remove worktree")
                    }
                }
            } else {
                match tokio::fs::remove_dir_all(&wt_path).await {
                    Ok(_) => {
                        tracing::info!(module = "cleanup", path = %wt_path.display(), "removed worktree dir (no repo context)")
                    }
                    Err(e) => {
                        tracing::warn!(module = "cleanup", path = %wt_path.display(), error = %e, "failed to remove worktree dir")
                    }
                }
            }
        }
    }

    if let Some(ref branch) = item.branch {
        if let Some(ref rp) = repo_path {
            if let Err(e) = super::git::delete_local_branch(rp, branch).await {
                tracing::warn!(module = "cleanup", branch = %branch, error = %e, "failed to delete branch");
            } else {
                tracing::info!(module = "cleanup", branch = %branch, "deleted branch");
            }
        }
    }

    if let Some(ref worker) = item.worker {
        let health_path = mando_config::worker_health_path();
        let mut health = super::health_store::load_health_state(&health_path);
        health.remove(worker.as_str());
        if let Err(e) = super::health_store::save_health_state(&health_path, &health) {
            tracing::warn!(path = %health_path.display(), error = %e, "failed to save health state during cleanup");
        }
    }

    {
        let id_str = item.id.to_string();
        let lock_dir = mando_config::state_dir().join("item-locks");
        let lock_path = lock_dir.join(format!("{id_str}.lock"));
        tokio::fs::remove_file(&lock_path).await.ok();
    }

    {
        let id_str = item.id.to_string();
        let timeline_path =
            super::timeline_store::timeline_path(&mando_config::state_dir(), &id_str);
        tokio::fs::remove_file(&timeline_path).await.ok();
    }

    {
        let ids_to_delete = [item.id.to_string(), item.best_id()];
        let mut total = 0u64;
        for id in &ids_to_delete {
            match mando_db::queries::sessions::delete_sessions_for_task(pool, id).await {
                Ok(n) => total += n,
                Err(e) => {
                    tracing::warn!(module = "cleanup", task_id = %id, error = %e, "failed to delete sessions");
                }
            }
        }
        if total > 0 {
            tracing::info!(module = "cleanup", item_id = %item.id, deleted = total, "purged session entries");
        }
    }

    // Close GitHub PR if requested
    if opts.close_pr {
        if let Some(ref pr) = item.pr {
            match mando_types::task::extract_pr_number(pr) {
                Some(pr_num) => {
                    // Prefer stored github_repo (captured at creation time), fall back to config
                    let repo = item.github_repo.clone().or_else(|| {
                        item.project
                            .as_deref()
                            .and_then(|name| mando_config::resolve_github_repo(Some(name), config))
                    });
                    match repo {
                        Some(repo) => match super::github::close_pr(&repo, pr_num).await {
                            Ok(()) => {
                                tracing::info!(module = "cleanup", pr = %pr, repo = %repo, "closed PR");
                            }
                            Err(e) => {
                                let msg = format!("Failed to close PR {pr}: {e}");
                                tracing::warn!(module = "cleanup", pr = %pr, error = %e, "failed to close PR");
                                warnings.push(msg);
                            }
                        },
                        None => {
                            let msg = format!(
                                "Cannot close PR {pr}: no github_repo configured for project"
                            );
                            tracing::warn!(module = "cleanup", pr = %pr, "{}", msg);
                            warnings.push(msg);
                        }
                    }
                }
                None => {
                    let msg = format!("Cannot close PR: malformed ref \"{pr}\"");
                    tracing::warn!(module = "cleanup", pr = %pr, "{}", msg);
                    warnings.push(msg);
                }
            }
        }
    }

    // Cancel Linear issue if requested
    if opts.cancel_linear {
        if let Some(ref linear_id) = item.linear_id {
            match super::linear::resolve_cli_path(&config.captain.linear_cli_path) {
                Ok(cli) => match super::linear::update_status(&cli, linear_id, "Canceled").await {
                    Ok(()) => {
                        tracing::info!(module = "cleanup", linear_id = %linear_id, "canceled Linear issue");
                    }
                    Err(e) => {
                        let msg = format!("Failed to cancel {linear_id}: {e}");
                        tracing::warn!(module = "cleanup", linear_id = %linear_id, error = %e, "failed to cancel Linear issue");
                        warnings.push(msg);
                    }
                },
                Err(e) => {
                    let msg = format!("Cannot cancel {linear_id}: Linear CLI not available ({e})");
                    tracing::warn!(module = "cleanup", error = %e, "Linear CLI not available");
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
