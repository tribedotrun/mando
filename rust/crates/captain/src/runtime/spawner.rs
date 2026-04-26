//! Worker spawning orchestrator — creates worktree, renders prompt, spawns CC.

use std::collections::HashMap;
use std::path::PathBuf;

use crate::Task;
use anyhow::{Context, Result};
use settings::CaptainWorkflow;
use settings::{CaptainConfig, ProjectConfig};

use crate::io::{hooks, pid_registry};

/// Credential to inject into the worker's environment.
pub(crate) struct WorkerCredential<'a> {
    pub id: i64,
    pub token: &'a str,
}

/// Spawn a new worker for a task.
#[tracing::instrument(skip_all)]
pub(crate) async fn spawn_worker(
    item: &Task,
    _project_slug: &str,
    project_config: &ProjectConfig,
    _captain_config: &CaptainConfig,
    workflow: &CaptainWorkflow,
    pool: &sqlx::SqlitePool,
    credential: Option<&WorkerCredential<'_>>,
) -> Result<SpawnResult> {
    let repo_path = global_infra::paths::expand_tilde(&project_config.path);

    // Worker name is task-scoped: worker-{taskId}-{seq}.
    // Uses worker_seq as-is (caller is responsible for incrementing before calling).
    let session_name = format!("worker-{}-{}", item.id, item.worker_seq);
    let session_id = global_infra::uuid::Uuid::v4().to_string();

    // Fetch origin so we branch off the latest remote HEAD.
    global_git::fetch_origin(&repo_path).await?;

    // Resolve worktree + branch via a pure plan (see `plan_worktree`).
    let stored_wt = item
        .worktree
        .as_deref()
        .map(global_infra::paths::expand_tilde);
    let default_branch = global_git::default_branch(&repo_path).await?;
    let (branch, wt_path) = match plan_worktree(stored_wt.as_ref(), item.branch.as_deref()) {
        WorktreePlan::Reuse { wt, branch } => {
            tracing::info!(
                module = "spawner",
                worktree = %wt.display(),
                branch = %branch,
                "reusing existing worktree for reopened item"
            );
            (branch, wt)
        }
        WorktreePlan::Rework { wt } => {
            // Rework: same worktree, fresh branch from origin/main.
            let slug = new_slug(item, next_worker_slot(&global_infra::paths::state_dir())?);
            let branch = format!("mando/{}", slug);
            tracing::info!(
                module = "spawner",
                worktree = %wt.display(),
                branch = %branch,
                "rework: resetting existing worktree to new branch"
            );
            global_git::reset_to_new_branch(&wt, &branch, &default_branch).await?;
            (branch, wt)
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
                module = "spawner",
                task_id = item.id,
                worktree = %wt.display(),
                stored_branch = ?stored_branch,
                "task worktree dir missing on disk — recreating at stored path to preserve invariant #4 (worktree permanent)"
            );
            let branch = recreate_worktree_at(item, &repo_path, &default_branch, &wt).await?;
            (branch, wt)
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
    };

    // Copy plan briefs into worktree if they exist (blocking fs → spawn_blocking).
    let discovered_plan = {
        let item_clone = item.clone();
        let wt_clone = wt_path.clone();
        tokio::task::spawn_blocking(move || copy_plan_briefs(&item_clone, &wt_clone)).await??
    };

    // Run pre_spawn hook.
    let hook_env = HashMap::new();
    hooks::pre_spawn(&project_config.hooks, &wt_path, &hook_env).await?;

    // Render prompt from workflow template (blocking fs → spawn_blocking).
    let prompt = {
        let item_clone = item.clone();
        let branch_clone = branch.clone();
        let wt_clone = wt_path.clone();
        let project_clone = project_config.clone();
        let workflow_clone = workflow.clone();
        tokio::task::spawn_blocking(move || {
            let slot = branch_slot(&branch_clone).unwrap_or(0);
            super::spawner_prompt::prepare_initial_worker_prompt(
                &item_clone,
                slot,
                &branch_clone,
                &wt_clone,
                &project_clone,
                &workflow_clone,
            )
        })
        .await??
    };

    // Create draft PR for PR-eligible tasks (scaffold commit + push + gh pr create).
    let mut draft_pr_number: Option<i64> = None;
    if !item.no_pr {
        match super::spawner_pr::create_draft_pr(item, &branch, &wt_path).await {
            Ok(pr_num) => {
                draft_pr_number = Some(pr_num);
                tracing::info!(
                    module = "spawner",
                    task_id = item.id,
                    pr_number = pr_num,
                    "created draft PR"
                );
            }
            Err(e) => {
                // Non-fatal: worker still starts, PR discovered later.
                tracing::warn!(
                    module = "spawner",
                    task_id = item.id,
                    error = %e,
                    "failed to create draft PR -- worker will start without it"
                );
            }
        }
    }

    // Spawn CC via mando-cc.
    let mut cc_builder = global_claude::CcConfig::builder()
        .model(&workflow.models.worker)
        .effort(global_claude::Effort::Max)
        .cwd(&wt_path)
        .session_id(&session_id)
        .caller("worker")
        .task_id(item.id.to_string())
        .worker_name(&session_name)
        .project(&item.project)
        .env("MANDO_TASK_ID", item.id.to_string());
    if let Some(cred) = credential {
        cc_builder = cc_builder.env("CLAUDE_CODE_OAUTH_TOKEN", cred.token);
    }
    let cc_config = cc_builder.build();

    let (child, pid, stream_path) =
        global_claude::spawn_detached(&cc_config, &prompt, &session_id).await?;
    crate::watch_worker_exit(child, pid, &session_id);

    // Write meta sidecar for retrospective debugging.
    global_claude::write_stream_meta(
        &global_claude::SessionMeta {
            session_id: &session_id,
            caller: "worker",
            task_id: &item.id.to_string(),
            worker_name: &session_name,
            project: &item.project,
            cwd: &wt_path.display().to_string(),
        },
        "running",
    );

    // Register PID in the session registry for liveness tracking.
    pid_registry::register(&session_id, pid)?;

    // Log "running" session entry so the UI shows it immediately.
    crate::io::headless_cc::log_running_session(
        pool,
        &session_id,
        &wt_path,
        "worker",
        &session_name,
        Some(item.id),
        false,
        credential.map(|c| c.id),
    )
    .await?;

    tracing::info!(
        module = "spawner",
        worker = %session_name,
        pid = %pid,
        title = %item.title,
        "spawned worker"
    );

    Ok(SpawnResult {
        session_name,
        session_id,
        pid,
        branch,
        worktree: wt_path.to_string_lossy().into_owned(),
        stream_path,
        plan: discovered_plan.or_else(|| item.plan.clone()),
        pr_number: draft_pr_number,
    })
}

/// Result of spawning a worker.
#[allow(dead_code)]
pub struct SpawnResult {
    pub session_name: String,
    pub session_id: String,
    pub pid: crate::Pid,
    pub branch: String,
    pub worktree: String,
    pub stream_path: PathBuf,
    /// Worktree-relative path to the plan/brief file, if one was found.
    pub plan: Option<String>,
    /// PR number if a draft PR was created during spawn.
    pub pr_number: Option<i64>,
}

/// Build the env overrides for a resumed worker process.
///
/// Uses the session's original credential if still healthy (not rate-limited,
/// not expired). Otherwise picks a fresh credential via load balancing.
/// Returns (env_map, credential_id_used).
#[tracing::instrument(skip_all)]
pub(crate) async fn credential_env_for_session(
    pool: &sqlx::SqlitePool,
    _session_id: &str,
) -> (std::collections::HashMap<String, String>, Option<i64>) {
    let mut env = std::collections::HashMap::new();
    // Prefer a freshly-picked healthy credential (pick_for_worker filters out
    // rate-limited ones). This ensures we rotate away from a rate-limited
    // credential on resume. Balance on worker sessions only.
    let fresh = super::tick_spawn::pick_credential(pool, Some("worker")).await;
    if let Some((cid, token)) = fresh {
        env.insert("CLAUDE_CODE_OAUTH_TOKEN".into(), token);
        return (env, Some(cid));
    }
    // No credentials configured -- fall through to ambient login.
    // (If all credentials are rate-limited, the tick spawn gate blocks the
    // reopen before reaching here.)
    (env, None)
}

use super::task_creation::next_worker_slot;

fn new_slug(item: &Task, slot: u64) -> String {
    format!("todo-{}-{}", item.id, slot)
}

/// Decision for how `spawn_worker` should resolve a task's worktree + branch.
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

/// Pure decision: which worktree plan should `spawn_worker` execute?
///
/// Separated so the branch selection is unit-testable without a real git
/// repo. All IO (next_worker_slot, git commands, logging) happens in the
/// caller based on the returned variant.
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

/// Recreate a missing worktree at its stored path with a fresh slot-derived
/// branch. Retries on `WorktreeAlreadyExists` (leftover metadata, counter
/// reuse, concurrent slot allocation) by rotating the branch slug; the path
/// itself stays pinned to `wt` so captain invariant #4 is preserved.
async fn recreate_worktree_at(
    item: &Task,
    repo_path: &std::path::Path,
    default_branch: &str,
    wt: &std::path::Path,
) -> Result<String> {
    const MAX_ATTEMPTS: usize = 20;

    for attempt in 0..MAX_ATTEMPTS {
        let slug = new_slug(item, next_worker_slot(&global_infra::paths::state_dir())?);
        let branch = format!("mando/{}", slug);

        if let Err(e) = global_git::prune_worktrees(repo_path).await {
            tracing::warn!(module = "spawner", error = %e, "failed to prune stale git worktree metadata before recreate");
        }
        if let Err(e) = global_git::delete_local_branch(repo_path, &branch).await {
            tracing::debug!(module = "spawner", branch = %branch, error = %e, "stale branch cleanup before recreate (expected if branch doesn't exist)");
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
                    module = "spawner",
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

fn branch_slot(branch: &str) -> Option<u64> {
    branch.rsplit('-').next()?.parse().ok()
}

/// Copy plan/brief files from `~/.mando/plans/` into the worktree's `.ai/briefs/`.
///
/// Copies any files related to the item's plan path. Only copies
/// files that don't already exist in the destination (idempotent).
/// Returns the worktree-relative brief path if one was found, or `None`.
/// Returns `Err` on filesystem failure so spawn_worker can abort cleanly
/// rather than starting a worker with missing plan files.
fn copy_plan_briefs(item: &Task, wt_path: &std::path::Path) -> Result<Option<String>> {
    let plans_dir = global_infra::paths::state_dir().join("plans");
    if !plans_dir.exists() {
        return Ok(None);
    }

    let briefs_dir = wt_path.join(".ai").join("briefs");

    std::fs::create_dir_all(&briefs_dir)
        .with_context(|| format!("failed to create briefs directory {}", briefs_dir.display()))?;

    // Look for a generic brief file matching the item ID.
    let id = &item.id.to_string();
    let brief_file = plans_dir.join(format!("item-{id}.md"));
    if brief_file.is_file() {
        let relative = format!(".ai/briefs/item-{id}.md");
        let dst = briefs_dir.join(format!("item-{id}.md"));
        if !dst.exists() {
            std::fs::copy(&brief_file, &dst).with_context(|| {
                format!(
                    "failed to copy item brief {} -> {}",
                    brief_file.display(),
                    dst.display()
                )
            })?;
        }
        return Ok(Some(relative));
    }
    Ok(None)
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
