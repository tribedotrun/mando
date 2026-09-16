//! Atomic task creation: reserves a git worktree, inserts a workbench
//! row, and inserts the task row all in one operation.
//!
//! The lifecycle invariant after this module lands: every task in the DB
//! has a real `workbench_id` pointing at a workbench whose `worktree`
//! points at a real on-disk git worktree. Spawn time only resumes the
//! worker against the existing worktree — it does not allocate slots,
//! create worktrees, or insert workbenches.
//!
//! On any failure between filesystem creation and DB commit, the
//! filesystem state is rolled back so the next attempt sees a clean
//! slate.
use std::path::{Path, PathBuf};

use crate::Task;
use anyhow::{Context, Result};
use settings::Config;

/// Insert a new task with a freshly-allocated workbench and on-disk
/// worktree. Returns the task id.
///
/// Filesystem creation runs first; DB inserts run inside a single
/// SQLite transaction. If the transaction fails, the on-disk worktree
/// and branch are cleaned up before the error is returned.
///
/// The `no_pr` / `githubRepo` requirement is intentionally NOT checked
/// here. `Task::new` defaults `no_pr = false` and the multipart task
/// route applies the user's `no_pr` flag in a follow-up
/// `update_fields` call after creation. Enforcing the github-repo
/// requirement at creation time would block legitimate
/// `no_pr = true` (research-only) tasks for repos without a configured
/// `githubRepo`. The check stays at spawn time
/// (`tick_spawn::spawn_worker_for_item`), where `no_pr` is already
/// settled.
#[tracing::instrument(skip_all, fields(project = %task.project, title = %task.title))]
pub async fn create_task_with_workbench(
    pool: &sqlx::SqlitePool,
    config: &Config,
    mut task: Task,
) -> Result<i64> {
    let (_slug, project_config) =
        settings::resolve_project_config(Some(task.project.as_str()), config).ok_or_else(|| {
            crate::TaskCreateError::UnknownProject {
                name: task.project.clone(),
                valid: config
                    .captain
                    .projects
                    .values()
                    .map(|pc| pc.name.clone())
                    .collect(),
            }
        })?;

    let repo_path = global_infra::paths::expand_tilde(&project_config.path);
    global_git::fetch_origin(&repo_path)
        .await
        .with_context(|| format!("fetch origin for {}", repo_path.display()))?;
    let default_branch = global_git::default_branch(&repo_path)
        .await
        .with_context(|| format!("resolve default branch for {}", repo_path.display()))?;

    let (branch, wt_path) = reserve_fresh_worktree(&task, &repo_path, &default_branch).await?;

    let result = insert_workbench_and_task_in_tx(pool, &mut task, &branch, &wt_path).await;

    match result {
        Ok(id) => Ok(id),
        Err(e) => {
            tracing::warn!(
                module = "task_creation",
                error = %e,
                worktree = %wt_path.display(),
                branch = %branch,
                "DB insert failed after worktree creation; cleaning up filesystem"
            );
            if let Err(rm) = global_git::remove_worktree(&repo_path, &wt_path).await {
                tracing::warn!(
                    module = "task_creation",
                    error = %rm,
                    "filesystem rollback: remove_worktree failed"
                );
            }
            if let Err(rm) = global_git::delete_local_branch(&repo_path, &branch).await {
                tracing::warn!(
                    module = "task_creation",
                    error = %rm,
                    "filesystem rollback: delete_local_branch failed"
                );
            }
            Err(e)
        }
    }
}

async fn insert_workbench_and_task_in_tx(
    pool: &sqlx::SqlitePool,
    task: &mut Task,
    branch: &str,
    wt_path: &Path,
) -> Result<i64> {
    let mut tx = pool.begin().await?;
    let now = global_types::now_rfc3339();
    let wb_title = if task.title.is_empty() {
        crate::workbench_title_now()
    } else {
        task.title.clone()
    };
    let wt_str = wt_path
        .to_str()
        .context("worktree path is not valid UTF-8")?;

    let wb_id = crate::io::queries::workbenches::insert_in_tx(
        &mut tx,
        task.project_id,
        wt_str,
        &wb_title,
        &now,
    )
    .await
    .context("INSERT workbenches")?;

    task.workbench_id = wb_id;
    task.worktree = Some(wt_str.to_string());
    task.branch = Some(branch.to_string());

    let task_id =
        crate::io::queries::tasks::insert_task_in_tx(&mut tx, task, "captain", task.source.clone())
            .await?;

    tx.commit().await?;
    Ok(task_id)
}

/// Reserve a fresh worktree for a task. Allocates a slot, computes the
/// path, runs `git worktree add -b <branch>`, and copies bootstrap files.
/// Retries on `WorktreeAlreadyExists` by rotating the slug.
#[tracing::instrument(skip_all, fields(repo = %repo_path.display(), task = %task.title))]
pub(crate) async fn reserve_fresh_worktree(
    task: &Task,
    repo_path: &Path,
    default_branch: &str,
) -> Result<(String, PathBuf)> {
    const MAX_ATTEMPTS: usize = 20;

    for attempt in 0..MAX_ATTEMPTS {
        let slot_state_dir = global_infra::paths::state_dir();
        let slot = tokio::task::spawn_blocking(move || next_worker_slot(&slot_state_dir)).await??;
        let slug = task_slug(task, slot);
        let branch = format!("mando/{}", slug);
        let wt = global_git::worktree_path(repo_path, &slug);

        if let Err(e) = global_git::prune_worktrees(repo_path).await {
            tracing::warn!(
                module = "task_creation",
                error = %e,
                "failed to prune stale worktree metadata"
            );
        }
        if let Err(e) = global_git::delete_local_branch(repo_path, &branch).await {
            tracing::debug!(
                module = "task_creation",
                branch = %branch,
                error = %e,
                "stale branch cleanup before create (expected if branch absent)"
            );
        }

        match global_git::create_worktree(repo_path, &branch, &wt, default_branch).await {
            Ok(()) => {
                crate::io::worktree_bootstrap::copy_local_files(repo_path, &wt).await;
                return Ok((branch, wt));
            }
            Err(e)
                if crate::find_git_error(&e).is_some_and(|g| {
                    matches!(g, crate::GitError::WorktreeAlreadyExists { .. })
                }) && attempt + 1 < MAX_ATTEMPTS =>
            {
                tracing::warn!(
                    module = "task_creation",
                    branch = %branch,
                    attempt = attempt + 1,
                    "branch/worktree already exists -- retrying with a fresh slot"
                );
            }
            Err(e) => return Err(e),
        }
    }

    anyhow::bail!(
        "failed to reserve a fresh worktree after {} attempts for task '{}'",
        MAX_ATTEMPTS,
        task.title
    )
}

/// Allocate the next monotonic worker slot from `<state_dir>/worker-counter.txt`.
pub(crate) fn next_worker_slot(state_dir: &Path) -> Result<u64> {
    let counter_path = state_dir.join("worker-counter.txt");
    let current = match std::fs::read_to_string(&counter_path) {
        Ok(contents) => contents.trim().parse::<u64>().map_err(|e| {
            anyhow::anyhow!(
                "corrupt worker counter file at {}: {:?} ({e}); refusing to reset to 0 because the counter must be monotonic",
                counter_path.display(),
                contents.trim()
            )
        })?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => 0,
        Err(e) => anyhow::bail!(
            "failed to read worker counter at {}: {e}",
            counter_path.display()
        ),
    };
    let next = current + 1;
    if let Some(parent) = counter_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&counter_path, next.to_string())?;
    Ok(next)
}

/// Compute a deterministic slug for a task at a given slot.
///
/// During task creation the task does not yet have an id (id is auto-assigned
/// by the INSERT). Use slot-only naming so the slug is unique and the worktree
/// path is computable before the row exists.
pub(crate) fn task_slug(task: &Task, slot: u64) -> String {
    if task.id > 0 {
        format!("todo-{}-{}", task.id, slot)
    } else {
        format!("todo-{}", slot)
    }
}
