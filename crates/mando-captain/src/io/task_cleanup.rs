//! Task cleanup — remove worktree, branch, health entry, Linear issue
//! when deleting tasks.

use anyhow::Result;
use mando_config::settings::Config;
use mando_types::Task;

pub(crate) async fn cleanup_task(
    item: &Task,
    config: &Config,
    pool: &sqlx::SqlitePool,
) -> Result<()> {
    let repo_path = item.project.as_deref().and_then(|name| {
        mando_config::resolve_project_config(Some(name), config)
            .map(|(_, pc)| mando_config::expand_tilde(&pc.path))
    });

    if let Some(ref worker) = item.worker {
        let pid = super::health_store::get_pid_for_worker(worker);
        if pid > 0 {
            mando_cc::kill_process(pid).await.ok();
            tracing::info!(module = "cleanup", worker = %worker, pid = pid, "killed worker");
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
            super::git::delete_local_branch(rp, branch).await.ok();
            tracing::info!(module = "cleanup", branch = %branch, "deleted branch");
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

    Ok(())
}

pub(crate) async fn cleanup_tasks(items: &[Task], config: &Config, pool: &sqlx::SqlitePool) {
    for item in items {
        if let Err(e) = cleanup_task(item, config, pool).await {
            tracing::warn!(module = "cleanup", title = %item.title, error = %e, "error cleaning up");
        }
    }
}
