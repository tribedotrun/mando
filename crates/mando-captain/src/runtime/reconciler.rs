//! Reconciler — resume incomplete operations on startup.

use anyhow::Result;
use mando_config::settings::Config;

use crate::io::ops_log::{self, OpsLog};
use crate::io::task_store::TaskStore;

pub async fn reconcile_on_startup(config: &Config, pool: &sqlx::SqlitePool) -> Result<()> {
    reconcile_orphaned_worktrees(config, pool).await;
    reconcile_worker_timeouts(pool).await;

    let log_path = ops_log::ops_log_path();
    let mut log = ops_log::load_ops_log(&log_path);

    ops_log::prune_stale(&mut log, ops_log::STALE_AGE_SECS);
    ops_log::save_ops_log(&log, &log_path)?;

    let incomplete = ops_log::incomplete_ops(&log);
    if incomplete.is_empty() {
        tracing::debug!(module = "reconciler", "no incomplete operations to resume");
        return Ok(());
    }

    tracing::info!(
        module = "reconciler",
        count = incomplete.len(),
        "found incomplete operations, resuming"
    );

    // Collect into owned tuples — one pass instead of three separate Vecs.
    let entries: Vec<(String, String, serde_json::Value)> = incomplete
        .iter()
        .map(|e| (e.op_id.clone(), e.op_type.clone(), e.params.clone()))
        .collect();

    for (op_id, op_type, params) in &entries {
        match op_type.as_str() {
            "merge" => {
                reconcile_merge(&mut log, op_id, params, config, pool).await?;
            }
            "accept" => {
                reconcile_accept(&mut log, op_id, params, pool).await?;
            }
            "todo_commit" => {
                reconcile_todo_commit(&mut log, op_id).await?;
            }
            "learn" => {
                reconcile_learn(&mut log, op_id).await?;
            }
            _ => {
                tracing::warn!(
                    module = "reconciler",
                    op_type = %op_type,
                    op_id = %op_id,
                    "unknown op_type, skipping"
                );
                ops_log::complete_op(&mut log, op_id);
            }
        }
    }

    ops_log::save_ops_log(&log, &log_path)?;
    tracing::info!(module = "reconciler", "reconciliation complete");
    Ok(())
}

/// Reset `worker_started_at` for all InProgress workers so daemon downtime
/// (crashes, restarts, laptop sleep) does not count toward the timeout budget.
async fn reconcile_worker_timeouts(pool: &sqlx::SqlitePool) {
    let store = TaskStore::new(pool.clone());
    let tasks = match store.load_all().await {
        Ok(t) => t,
        Err(e) => {
            tracing::error!(module = "reconciler", error = %e,
                "failed to load tasks — skipping worker timeout reconciliation");
            return;
        }
    };
    let now = mando_types::now_rfc3339();
    let mut count = 0u32;
    for task in &tasks {
        if task.status == mando_types::task::ItemStatus::InProgress
            && task.worker_started_at.is_some()
        {
            if let Err(e) = store
                .update(task.id, |t| {
                    t.worker_started_at = Some(now.clone());
                })
                .await
            {
                tracing::warn!(module = "reconciler", task_id = task.id, error = %e,
                    "failed to reset worker_started_at on startup");
            } else {
                count += 1;
            }
        }
    }
    if count > 0 {
        tracing::info!(
            module = "reconciler",
            count,
            "reset worker timeout clocks on startup"
        );
    }
}

async fn reconcile_orphaned_worktrees(config: &Config, pool: &sqlx::SqlitePool) {
    let store = TaskStore::new(pool.clone());

    let mut tracked: std::collections::HashSet<std::path::PathBuf> =
        std::collections::HashSet::new();
    let tasks = match store.load_all().await {
        Ok(tasks) => tasks,
        Err(e) => {
            tracing::error!(
                module = "reconciler",
                error = %e,
                "failed to load tasks — skipping orphan worktree cleanup to avoid destroying work"
            );
            return;
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
                    "failed to list git worktrees — skipping orphan cleanup for this project"
                );
            }
        }
    }

    let worktrees_dir = crate::io::git::worktrees_dir();
    if !worktrees_dir.is_dir() {
        return;
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
            return;
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
            Ok(_) => tracing::info!(
                module = "reconciler",
                path = %path.display(),
                "removed orphaned worktree on startup"
            ),
            Err(e) => tracing::warn!(
                module = "reconciler",
                path = %path.display(),
                error = %e,
                "failed to remove orphaned worktree"
            ),
        }
    }
}

async fn reconcile_merge(
    log: &mut OpsLog,
    op_id: &str,
    params: &serde_json::Value,
    config: &Config,
    pool: &sqlx::SqlitePool,
) -> Result<()> {
    let pr = params["pr"].as_str().unwrap_or("");
    let repo = params["repo"].as_str().unwrap_or("");
    let item_id = params["item_id"].as_str().unwrap_or("");

    if pr.is_empty() || repo.is_empty() {
        ops_log::abandon_op(log, op_id, "merge WAL entry has missing pr or repo params");
        return Ok(());
    }

    tracing::info!(module = "reconciler", pr = %pr, item_id = %item_id, "resuming merge");

    // Always re-check GitHub state (even if check_merged was set on a previous run)
    // to avoid stale data — the PR may have been merged between restarts.
    {
        let output = tokio::process::Command::new("gh")
            .args(["pr", "view", pr, "--json", "state", "--repo", repo])
            .output()
            .await;

        match output {
            Ok(output) => match serde_json::from_slice::<serde_json::Value>(&output.stdout) {
                Ok(json) => match json["state"].as_str() {
                    Some("MERGED") => {
                        ops_log::mark_step(log, op_id, "squash_merge");
                        ops_log::mark_step(log, op_id, "check_merged");
                    }
                    Some("OPEN") | Some("CLOSED") => {
                        ops_log::mark_step(log, op_id, "check_merged");
                    }
                    other => {
                        tracing::warn!(
                            module = "reconciler",
                            pr = %pr,
                            state = ?other,
                            "unexpected PR state from GitHub — will retry"
                        );
                        return Ok(());
                    }
                },
                Err(e) => {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    tracing::warn!(
                        module = "reconciler",
                        pr = %pr,
                        error = %e,
                        stderr = %stderr,
                        "gh pr view returned non-JSON — will retry"
                    );
                    return Ok(());
                }
            },
            Err(e) => {
                tracing::warn!(
                    module = "reconciler",
                    pr = %pr,
                    error = %e,
                    "gh pr view failed — will retry check_merged on next reconciliation"
                );
                return Ok(());
            }
        }
    }

    // PR was not actually merged on GitHub — abandon this WAL entry.
    if !ops_log::is_step_done(log, op_id, "squash_merge") {
        tracing::warn!(
            module = "reconciler",
            pr = %pr,
            item_id = %item_id,
            "PR is not merged on GitHub — abandoning stale merge WAL entry"
        );
        ops_log::abandon_op(log, op_id, &format!("PR {pr} not merged on GitHub"));
        return Ok(());
    }

    if !ops_log::is_step_done(log, op_id, "update_task") {
        let store = TaskStore::new(pool.clone());
        let item_id_num: i64 = item_id.parse().unwrap_or_else(|e| {
            tracing::warn!(raw = item_id, error = %e, "invalid item_id in merge ops log, falling back to PR lookup");
            0
        });
        let match_id = if item_id_num > 0 {
            store
                .find_by_id(item_id_num)
                .await
                .ok()
                .flatten()
                .map(|_| item_id_num)
        } else {
            None
        };
        let match_id = match match_id {
            Some(id) => Some(id),
            None => match store.load_all().await {
                Ok(tasks) => {
                    let pr_num = match mando_types::task::extract_pr_number(pr) {
                        Some(n) => n,
                        None => {
                            tracing::warn!(module = "reconciler", pr = %pr, "unparseable PR ref in merge WAL — abandoning");
                            ops_log::abandon_op(log, op_id, "unparseable PR ref");
                            return Ok(());
                        }
                    };
                    tasks
                        .iter()
                        .find(|t| {
                            t.pr.as_deref().is_some_and(|stored| {
                                mando_types::task::extract_pr_number(stored) == Some(pr_num)
                            })
                        })
                        .map(|t| t.id)
                }

                Err(e) => {
                    tracing::warn!(module = "reconciler", error = %e, "failed to load tasks for PR lookup");
                    None
                }
            },
        };
        if let Some(id) = match_id {
            if let Err(e) = store
                .update(id, |t| {
                    t.status = mando_types::task::ItemStatus::Merged;
                })
                .await
            {
                tracing::warn!(
                    module = "reconciler",
                    item_id = %item_id,
                    error = %e,
                    "failed to update task status to Merged — will retry"
                );
                return Ok(());
            }
            super::timeline_emit::emit(
                pool,
                id,
                mando_types::timeline::TimelineEventType::Merged,
                "reconciler",
                &format!("Reconciler confirmed PR {pr} merged on GitHub"),
                serde_json::json!({ "pr": pr, "source": "reconciler" }),
            )
            .await;
        }
        ops_log::mark_step(log, op_id, "update_task");
    }

    if !ops_log::is_step_done(log, op_id, "post_merge_hook") {
        if let Some((_, project_config)) = mando_config::resolve_project_config(Some(repo), config)
        {
            let repo_path = mando_config::expand_tilde(&project_config.path);
            if let Err(e) = crate::io::hooks::post_merge(
                &project_config.hooks,
                &repo_path,
                &std::collections::HashMap::new(),
            )
            .await
            {
                tracing::warn!(
                    module = "reconciler",
                    pr = %pr,
                    error = %e,
                    "post-merge hook failed"
                );
            }
        }
        ops_log::mark_step(log, op_id, "post_merge_hook");
    }

    ops_log::complete_op(log, op_id);
    Ok(())
}

async fn reconcile_accept(
    log: &mut OpsLog,
    op_id: &str,
    params: &serde_json::Value,
    pool: &sqlx::SqlitePool,
) -> Result<()> {
    let item_id = params["item_id"].as_str().unwrap_or("");
    tracing::info!(module = "reconciler", item_id = %item_id, "resuming accept");

    if !ops_log::is_step_done(log, op_id, "update_task") {
        let store = TaskStore::new(pool.clone());
        let id: i64 = item_id.parse().unwrap_or_else(|e| {
            tracing::warn!(raw = item_id, error = %e, "invalid item_id in accept ops log");
            0
        });
        if id > 0 {
            if let Err(e) = store
                .update(id, |t| {
                    t.status = mando_types::task::ItemStatus::CompletedNoPr;
                })
                .await
            {
                tracing::warn!(
                    module = "reconciler",
                    item_id = %item_id,
                    error = %e,
                    "failed to update task status to CompletedNoPr — will retry"
                );
                return Ok(());
            }
        }
        ops_log::mark_step(log, op_id, "update_task");
    }

    ops_log::complete_op(log, op_id);
    Ok(())
}

async fn reconcile_todo_commit(log: &mut OpsLog, op_id: &str) -> Result<()> {
    tracing::info!(module = "reconciler", op_id = %op_id, "resuming todo_commit");
    ops_log::complete_op(log, op_id);
    Ok(())
}

async fn reconcile_learn(log: &mut OpsLog, op_id: &str) -> Result<()> {
    tracing::info!(module = "reconciler", op_id = %op_id, "resuming learn");
    ops_log::complete_op(log, op_id);
    Ok(())
}
