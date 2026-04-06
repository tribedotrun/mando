//! Orphaned worktree detection and cleanup on startup.

use mando_config::settings::Config;

use crate::io::task_store::TaskStore;

#[derive(Debug, Default)]
pub(crate) struct OrphanReconcileReport {
    pub load_tasks_error: Option<String>,
    pub list_worktrees_errors: Vec<String>,
    pub read_dir_error: Option<String>,
    pub removed: Vec<String>,
    pub remove_errors: Vec<String>,
}

impl OrphanReconcileReport {
    pub fn has_problems(&self) -> bool {
        self.load_tasks_error.is_some()
            || !self.list_worktrees_errors.is_empty()
            || self.read_dir_error.is_some()
            || !self.remove_errors.is_empty()
    }
}

pub(crate) async fn reconcile_orphaned_worktrees(
    config: &Config,
    pool: &sqlx::SqlitePool,
) -> OrphanReconcileReport {
    let mut report = OrphanReconcileReport::default();
    let store = TaskStore::new(pool.clone());

    let mut tracked: std::collections::HashSet<std::path::PathBuf> =
        std::collections::HashSet::new();
    let tasks = match store.load_all().await {
        Ok(tasks) => tasks,
        Err(e) => {
            tracing::error!(
                module = "reconciler",
                error = %e,
                "failed to load tasks, skipping orphan worktree cleanup to avoid destroying work"
            );
            report.load_tasks_error = Some(e.to_string());
            return report;
        }
    };
    for task in tasks {
        if let Some(ref wt) = task.worktree {
            tracked.insert(mando_config::expand_tilde(wt));
        }
    }

    // Collect git-tracked worktrees from ALL projects before cleanup,
    // so we never delete a worktree that belongs to another project.
    let mut all_git_tracked = std::collections::HashSet::new();
    let mut project_prefixes = Vec::new();
    for project_cfg in config.captain.projects.values() {
        let project_path = mando_config::expand_tilde(&project_cfg.path);
        let repo_name = project_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("repo")
            .to_string();
        match crate::io::git::list_worktrees(&project_path).await {
            Ok(paths) => {
                all_git_tracked.extend(paths);
                project_prefixes.push((project_path, format!("{repo_name}-")));
            }
            Err(e) => {
                tracing::error!(
                    module = "reconciler",
                    project = %project_path.display(),
                    error = %e,
                    "failed to list git worktrees, skipping orphan cleanup for this project"
                );
                report
                    .list_worktrees_errors
                    .push(format!("{}: {e}", project_path.display()));
            }
        }
    }

    let worktrees_dir = crate::io::git::worktrees_dir();
    if !worktrees_dir.is_dir() {
        return report;
    }
    let mut entries = match tokio::fs::read_dir(&worktrees_dir).await {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!(
                module = "reconciler",
                path = %worktrees_dir.display(),
                error = %e,
                "failed to read worktrees directory"
            );
            report.read_dir_error = Some(format!("{}: {e}", worktrees_dir.display()));
            return report;
        }
    };

    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let dir_name = entry.file_name().to_string_lossy().to_string();
        // Find the owning project (longest prefix wins).
        let owner = project_prefixes
            .iter()
            .filter(|(_, pfx)| dir_name.starts_with(pfx.as_str()))
            .max_by_key(|(_, pfx)| pfx.len());
        let Some((project_path, _)) = owner else {
            continue;
        };
        if tracked.contains(&path) || all_git_tracked.contains(&path) {
            continue;
        }
        match crate::io::git::remove_worktree(project_path, &path).await {
            Ok(_) => {
                tracing::info!(
                    module = "reconciler",
                    path = %path.display(),
                    "removed orphaned worktree on startup"
                );
                report.removed.push(path.display().to_string());
            }
            Err(e) => {
                tracing::warn!(
                    module = "reconciler",
                    path = %path.display(),
                    error = %e,
                    "failed to remove orphaned worktree"
                );
                report
                    .remove_errors
                    .push(format!("{}: {e}", path.display()));
            }
        }
    }

    report
}
