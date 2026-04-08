//! Worker spawning orchestrator — creates worktree, renders prompt, spawns CC.

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{Context, Result};
use mando_config::settings::{CaptainConfig, ProjectConfig};
use mando_config::workflow::CaptainWorkflow;
use mando_types::Task;
use rustc_hash::FxHashMap;

use crate::io::{git, hooks, pid_registry};

/// Spawn a new worker for a task.
///
/// Steps:
/// 1. Allocate a worker slot (monotonic counter).
/// 2. Create git worktree with a new branch.
/// 3. Run pre_spawn hook.
/// 4. Render the initial prompt.
/// 5. Spawn CC subprocess.
/// 6. Record PID and session ID.
pub(crate) async fn spawn_worker(
    item: &Task,
    _project_slug: &str,
    project_config: &ProjectConfig,
    _captain_config: &CaptainConfig,
    workflow: &CaptainWorkflow,
    pool: &sqlx::SqlitePool,
) -> Result<SpawnResult> {
    let repo_path = mando_config::expand_tilde(&project_config.path);

    // Worker name is task-scoped: worker-{taskId}-{seq}.
    // Uses worker_seq as-is (caller is responsible for incrementing before calling).
    let session_name = format!("worker-{}-{}", item.id, item.worker_seq);
    let session_id = mando_uuid::Uuid::v4().to_string();

    // Fetch origin so we branch off the latest remote HEAD.
    git::fetch_origin(&repo_path).await?;

    // Reuse existing worktree/branch if present (e.g. reopened item), otherwise create new.
    let existing_wt = item
        .worktree
        .as_deref()
        .map(mando_config::expand_tilde)
        .filter(|p| p.exists());
    let (branch, wt_path) =
        if let (Some(wt), Some(existing_branch)) = (existing_wt, item.branch.as_deref()) {
            tracing::info!(
                module = "spawner",
                worktree = %wt.display(),
                branch = existing_branch,
                "reusing existing worktree for reopened item"
            );
            (existing_branch.to_string(), wt)
        } else {
            let default_branch = git::default_branch(&repo_path).await?;
            reserve_fresh_worktree(item, &repo_path, &default_branch).await?
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
            prepare_initial_worker_prompt(
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

    // Spawn CC via mando-cc.
    let mut cc_builder = mando_cc::CcConfig::builder()
        .model(&workflow.models.worker)
        .effort(mando_cc::Effort::Max)
        .cwd(&wt_path)
        .session_id(&session_id)
        .caller("worker")
        .task_id(item.id.to_string())
        .worker_name(&session_name)
        .project(&item.project);
    if let Some(ref fb) = workflow.models.fallback {
        cc_builder = cc_builder.fallback_model(fb);
    }
    let cc_config = cc_builder.build();

    let (child, pid, stream_path) =
        mando_cc::spawn_detached(&cc_config, &prompt, &session_id).await?;
    crate::watch_worker_exit(child, pid, &session_id);

    // Write meta sidecar for retrospective debugging.
    mando_cc::write_stream_meta(
        &mando_cc::SessionMeta {
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
    })
}

/// Result of spawning a worker.
pub struct SpawnResult {
    pub session_name: String,
    pub session_id: String,
    pub pid: mando_types::Pid,
    pub branch: String,
    pub worktree: String,
    pub stream_path: PathBuf,
    /// Worktree-relative path to the plan/brief file, if one was found.
    pub plan: Option<String>,
}

fn next_worker_slot(state_dir: &std::path::Path) -> Result<u64> {
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

fn new_slug(item: &Task, slot: u64) -> String {
    format!("todo-{}-{}", item.id, slot)
}

async fn reserve_fresh_worktree(
    item: &Task,
    repo_path: &std::path::Path,
    default_branch: &str,
) -> Result<(String, PathBuf)> {
    const MAX_ATTEMPTS: usize = 20;

    for attempt in 0..MAX_ATTEMPTS {
        let slot_state_dir = mando_config::state_dir();
        let slot = tokio::task::spawn_blocking(move || next_worker_slot(&slot_state_dir)).await??;
        let slug = new_slug(item, slot);
        let branch = format!("mando/{}", slug);
        let wt = git::worktree_path(repo_path, &slug);

        if let Err(e) = git::prune_worktrees(repo_path).await {
            tracing::warn!(module = "spawner", error = %e, "failed to prune stale git worktrees");
        }
        if let Err(e) = git::delete_local_branch(repo_path, &branch).await {
            tracing::warn!(
                module = "spawner",
                branch = %branch,
                error = %e,
                "failed to delete stale local branch"
            );
        }

        match git::create_worktree(repo_path, &branch, &wt, default_branch).await {
            Ok(()) => return Ok((branch, wt)),
            Err(e) if e.to_string().contains("already exists") && attempt + 1 < MAX_ATTEMPTS => {
                tracing::warn!(
                    module = "spawner",
                    branch = %branch,
                    attempt = attempt + 1,
                    "branch/worktree already exists — retrying with a fresh slot"
                );
            }
            Err(e) => return Err(e),
        }
    }

    anyhow::bail!(
        "failed to reserve a fresh worktree after {} attempts for task {}",
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
    let plans_dir = mando_config::state_dir().join("plans");
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

fn prepare_initial_worker_prompt(
    item: &Task,
    slot: u64,
    branch: &str,
    wt_path: &std::path::Path,
    project_config: &ProjectConfig,
    workflow: &CaptainWorkflow,
) -> Result<String> {
    let plan = resolve_worker_plan_path(item, wt_path)?;
    let is_adopted = is_adopted_handoff(item, plan.as_deref(), wt_path);
    let prompt_name = if is_adopted {
        "worker_continue"
    } else if plan.is_some() {
        "worker_briefed"
    } else {
        "worker_initial"
    };
    let initial_prompt_name = if is_adopted { "adopted" } else { "worker" };

    let context = item.context.as_deref().unwrap_or("");
    let original_prompt = item.original_prompt.as_deref().unwrap_or("");
    let task_id_str = item.id.to_string();
    let no_pr = if item.no_pr { "true" } else { "" };
    let workpad_path = ensure_workpad_path(item)?;

    let check_command = check_command_or_fallback(project_config);

    let mut brief_vars: FxHashMap<&str, String> = FxHashMap::default();
    brief_vars.insert("title", item.title.clone());
    brief_vars.insert("context", context.to_string());
    brief_vars.insert("branch", branch.to_string());
    brief_vars.insert("id", task_id_str.clone());
    brief_vars.insert("original_prompt", original_prompt.to_string());
    brief_vars.insert("worker_preamble", project_config.worker_preamble.clone());
    brief_vars.insert("check_command", check_command.clone());
    brief_vars.insert("no_pr", no_pr.to_string());
    brief_vars.insert("workpad_path", workpad_path.clone());
    if let Some(ref plan_path) = plan {
        brief_vars.insert("plan", plan_path.clone());
    }

    let rendered_brief = mando_config::render_prompt(prompt_name, &workflow.prompts, &brief_vars)
        .map_err(anyhow::Error::msg)?;

    let brief_filename = worker_brief_filename(item, slot);
    let briefs_dir = wt_path.join(".ai").join("briefs");
    std::fs::create_dir_all(&briefs_dir)?;
    let brief_path = briefs_dir.join(&brief_filename);
    std::fs::write(&brief_path, rendered_brief)?;

    let mut vars: FxHashMap<&str, String> = FxHashMap::default();
    vars.insert("brief_filename", brief_filename.clone());
    vars.insert("brief_path", brief_path.display().to_string());
    vars.insert("id", task_id_str);
    vars.insert("no_pr", no_pr.to_string());
    vars.insert("workpad_path", workpad_path);

    mando_config::render_initial_prompt(initial_prompt_name, &workflow.initial_prompts, &vars)
        .map_err(anyhow::Error::msg)
}

fn ensure_workpad_path(item: &Task) -> Result<String> {
    let workpad_path = mando_config::data_dir()
        .join("plans")
        .join(item.id.to_string())
        .join("workpad.md");
    if let Some(parent) = workpad_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create workpad directory {}", parent.display()))?;
    }
    if !workpad_path.exists() {
        std::fs::write(&workpad_path, "")
            .with_context(|| format!("failed to initialize workpad {}", workpad_path.display()))?;
    }
    Ok(workpad_path.display().to_string())
}

fn worker_brief_filename(item: &Task, slot: u64) -> String {
    format!("todo-{}-{slot}.md", item.id)
}

fn resolve_worker_plan_path(item: &Task, wt_path: &std::path::Path) -> Result<Option<String>> {
    let Some(plan_path) = item.plan.as_deref() else {
        return Ok(None);
    };

    let plan = mando_config::expand_tilde(plan_path);
    if plan.is_absolute() {
        let file_name = plan
            .file_name()
            .ok_or_else(|| anyhow::anyhow!("plan path has no filename: {}", plan.display()))?;
        let briefs_dir = wt_path.join(".ai").join("briefs");
        std::fs::create_dir_all(&briefs_dir)?;
        let copied_path = briefs_dir.join(file_name);
        if plan != copied_path {
            std::fs::copy(&plan, &copied_path)?;
        }
        return Ok(Some(copied_path.display().to_string()));
    }

    let relative = plan_path.trim().to_string();
    if relative.is_empty() {
        return Ok(None);
    }

    Ok(Some(wt_path.join(relative).display().to_string()))
}

fn is_adopted_handoff(item: &Task, plan: Option<&str>, wt_path: &std::path::Path) -> bool {
    plan.is_some_and(|path| path.ends_with("adopt-handoff.md"))
        && item.worktree.is_some()
        && item.branch.is_some()
        && wt_path.exists()
}

const CHECK_COMMAND_FALLBACK: &str =
    "the project's quality gate (formatting, linting, tests — check CLAUDE.md for the exact command)";

fn check_command_or_fallback(project_config: &ProjectConfig) -> String {
    if project_config.check_command.is_empty() {
        CHECK_COMMAND_FALLBACK.to_string()
    } else {
        format!("`{}`", project_config.check_command)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_worktree() -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!("mando-spawner-{}", mando_uuid::Uuid::v4()));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn generic_items_render_worker_initial_brief() {
        let wt = temp_worktree();
        let mut item = Task::new("Fix auth redirect");
        item.context = Some("Auth redirect loop in login callback".into());
        let workflow = CaptainWorkflow::compiled_default();
        let project = ProjectConfig::default();

        let initial =
            prepare_initial_worker_prompt(&item, 1, "mando/fix-auth-1", &wt, &project, &workflow)
                .unwrap();

        assert!(initial.contains(&wt.join(".ai/briefs/todo-0-1.md").display().to_string()));
        assert!(initial.contains("Before you update the workpad, first read"));
        assert!(initial.contains(
            &mando_config::data_dir()
                .join("plans/0/workpad.md")
                .display()
                .to_string()
        ));
        let brief = std::fs::read_to_string(wt.join(".ai/briefs/todo-0-1.md")).unwrap();
        assert!(brief.contains("Captain Brief"));
        assert!(brief.contains("Auth redirect loop in login callback"));
        assert!(brief.contains(
            &mando_config::data_dir()
                .join("plans/0/workpad.md")
                .display()
                .to_string()
        ));
    }

    #[test]
    fn planned_items_render_worker_briefed_flow() {
        let wt = temp_worktree();
        let plan_source = wt.join("source-brief.md");
        std::fs::write(&plan_source, "# Brief").unwrap();

        let mut item = Task::new("Implement planned change");
        item.plan = Some(plan_source.display().to_string());

        let workflow = CaptainWorkflow::compiled_default();
        let project = ProjectConfig::default();

        prepare_initial_worker_prompt(&item, 2, "mando/todo-0", &wt, &project, &workflow).unwrap();

        let brief = std::fs::read_to_string(wt.join(".ai/briefs/todo-0-2.md")).unwrap();
        assert!(brief.contains("Human-Curated Plan"));
        assert!(brief.contains(&wt.join(".ai/briefs/source-brief.md").display().to_string()));
        assert!(wt.join(".ai/briefs/source-brief.md").exists());
    }

    #[test]
    fn tilde_plan_paths_are_copied_into_worktree_briefs() {
        let wt = temp_worktree();
        let home = std::path::PathBuf::from(std::env::var("HOME").unwrap());
        let plan_source = home.join(format!(
            ".mando/plans/tilde-test-{}/brief.md",
            mando_uuid::Uuid::v4()
        ));
        std::fs::create_dir_all(plan_source.parent().unwrap()).unwrap();
        std::fs::write(&plan_source, "# Brief from tilde path").unwrap();

        let mut item = Task::new("Implement planned change");
        item.plan = Some(format!(
            "~/.mando/plans/{}/brief.md",
            plan_source
                .parent()
                .and_then(|path| path.file_name())
                .unwrap()
                .to_string_lossy()
        ));

        let workflow = CaptainWorkflow::compiled_default();
        let project = ProjectConfig::default();

        prepare_initial_worker_prompt(&item, 4, "mando/todo-0", &wt, &project, &workflow).unwrap();

        let brief = std::fs::read_to_string(wt.join(".ai/briefs/todo-0-4.md")).unwrap();
        assert!(brief.contains(&wt.join(".ai/briefs/brief.md").display().to_string()));
        assert!(wt.join(".ai/briefs/brief.md").exists());

        let _ = std::fs::remove_file(&plan_source);
        let _ = std::fs::remove_dir(plan_source.parent().unwrap());
    }

    #[test]
    fn adopted_items_render_worker_continue_flow() {
        let wt = temp_worktree();
        let adopt_brief = wt.join(".ai/briefs/adopt-handoff.md");
        std::fs::create_dir_all(adopt_brief.parent().unwrap()).unwrap();
        std::fs::write(&adopt_brief, "# Adopt handoff").unwrap();

        let mut item = Task::new("Finish in-flight work");
        item.plan = Some(".ai/briefs/adopt-handoff.md".into());
        item.worktree = Some(wt.display().to_string());
        item.branch = Some("feature/adopt".into());

        let workflow = CaptainWorkflow::compiled_default();
        let project = ProjectConfig::default();

        let initial =
            prepare_initial_worker_prompt(&item, 3, "feature/adopt", &wt, &project, &workflow)
                .unwrap();

        assert!(initial.contains("handed off"));
        assert!(initial.contains("Before you update the workpad, first read"));
        assert!(initial.contains(
            &mando_config::data_dir()
                .join("plans/0/workpad.md")
                .display()
                .to_string()
        ));
        let brief = std::fs::read_to_string(wt.join(".ai/briefs/todo-0-3.md")).unwrap();
        assert!(brief.contains("Mid-Implementation Handoff"));
    }

    // ── is_adopted_handoff boundary conditions ──────────────────────

    #[test]
    fn adopt_requires_all_four_conditions() {
        let wt = temp_worktree();

        // All conditions met → adopted
        let mut item = Task::new("test");
        item.worktree = Some(wt.display().to_string());
        item.branch = Some("b".into());
        assert!(is_adopted_handoff(
            &item,
            Some(".ai/briefs/adopt-handoff.md"),
            &wt
        ));

        // Missing worktree → not adopted
        let mut item2 = Task::new("test");
        item2.branch = Some("b".into());
        assert!(!is_adopted_handoff(
            &item2,
            Some(".ai/briefs/adopt-handoff.md"),
            &wt
        ));

        // Missing branch → not adopted
        let mut item3 = Task::new("test");
        item3.worktree = Some(wt.display().to_string());
        assert!(!is_adopted_handoff(
            &item3,
            Some(".ai/briefs/adopt-handoff.md"),
            &wt
        ));

        // Wrong plan filename → not adopted
        let mut item4 = Task::new("test");
        item4.worktree = Some(wt.display().to_string());
        item4.branch = Some("b".into());
        assert!(!is_adopted_handoff(
            &item4,
            Some(".ai/briefs/regular-brief.md"),
            &wt
        ));

        // No plan → not adopted
        let mut item5 = Task::new("test");
        item5.worktree = Some(wt.display().to_string());
        item5.branch = Some("b".into());
        assert!(!is_adopted_handoff(&item5, None, &wt));

        // Nonexistent worktree path → not adopted
        let mut item6 = Task::new("test");
        item6.worktree = Some("/nonexistent/path".into());
        item6.branch = Some("b".into());
        assert!(!is_adopted_handoff(
            &item6,
            Some(".ai/briefs/adopt-handoff.md"),
            &std::path::PathBuf::from("/nonexistent/path")
        ));
    }
}
