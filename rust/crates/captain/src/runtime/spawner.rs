//! Worker spawning orchestrator — creates worktree, renders prompt, spawns CC.

use std::collections::HashMap;

use crate::Task;
use anyhow::{Context, Result};
use settings::CaptainWorkflow;
use settings::ProjectConfig;

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
    project_config: &ProjectConfig,
    workflow: &CaptainWorkflow,
    pool: &sqlx::SqlitePool,
    credential: Option<&WorkerCredential<'_>>,
    worker_model: &str,
) -> Result<SpawnResult> {
    // Worker name is task-scoped: worker-{taskId}-{seq}.
    // Uses worker_seq as-is (caller is responsible for incrementing before calling).
    let session_name = format!("worker-{}-{}", item.id, item.worker_seq);
    let session_id = global_infra::uuid::Uuid::v4().to_string();

    let (branch, wt_path) =
        super::worker_checkout::resolve_worker_checkout(project_config, item, "spawner").await?;

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
        .model(worker_model)
        .effort(workflow.agent.cc_effort)
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

    let (child, pid, _stream_path) =
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
        worker_model,
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
        branch,
        worktree: wt_path.to_string_lossy().into_owned(),
        plan: discovered_plan.or_else(|| item.plan.clone()),
        pr_number: draft_pr_number,
    })
}

/// Result of spawning a worker.
pub struct SpawnResult {
    pub session_name: String,
    pub session_id: String,
    pub branch: String,
    pub worktree: String,
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
    // credential on resume. Balancing is over the single global pool.
    let fresh = super::tick_spawn::pick_credential(pool).await;
    if let Some((cid, token)) = fresh {
        env.insert("CLAUDE_CODE_OAUTH_TOKEN".into(), token);
        return (env, Some(cid));
    }
    // No credentials configured -- fall through to ambient login.
    // (If all credentials are rate-limited, the tick spawn gate blocks the
    // reopen before reaching here.)
    (env, None)
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
