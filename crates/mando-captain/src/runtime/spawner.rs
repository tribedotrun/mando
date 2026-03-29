//! Worker spawning orchestrator — creates worktree, renders prompt, spawns CC.

use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::Result;
use mando_config::settings::{CaptainConfig, ProjectConfig};
use mando_config::workflow::CaptainWorkflow;
use mando_types::Task;

use crate::io::{git, health_store, hooks};

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
    let state_dir = mando_config::state_dir();

    // Allocate slot (global counter for worktree/branch slug uniqueness).
    let slot = next_worker_slot(&state_dir)?;
    // Worker name is task-scoped: worker-{taskId}-{seq}.
    // Uses worker_seq as-is (caller is responsible for incrementing before calling).
    let session_name = format!("worker-{}-{}", item.id, item.worker_seq);
    let session_id = mando_uuid::Uuid::v4().to_string();

    // Fetch origin so we branch off the latest remote HEAD.
    git::fetch_origin(&repo_path).await?;

    // Reuse existing worktree/branch if present (e.g. reopened item), otherwise create new.
    let (branch, wt_path) = if let (Some(existing_wt), Some(existing_branch)) =
        (item.worktree.as_deref(), item.branch.as_deref())
    {
        let wt = mando_config::expand_tilde(existing_wt);
        if wt.exists() {
            tracing::info!(
                module = "spawner",
                worktree = %wt.display(),
                branch = existing_branch,
                "reusing existing worktree for reopened item"
            );
            (existing_branch.to_string(), wt)
        } else {
            let slug = new_slug(item, slot);
            let branch = format!("mando/{}", slug);
            let wt = git::worktree_path(&repo_path, &slug);
            let default_branch = git::default_branch(&repo_path).await?;
            git::delete_local_branch(&repo_path, &branch).await.ok();
            git::create_worktree(&repo_path, &branch, &wt, &default_branch).await?;
            (branch, wt)
        }
    } else {
        let slug = new_slug(item, slot);
        let branch = format!("mando/{}", slug);
        let wt = git::worktree_path(&repo_path, &slug);
        let default_branch = git::default_branch(&repo_path).await?;
        git::delete_local_branch(&repo_path, &branch).await.ok();
        git::create_worktree(&repo_path, &branch, &wt, &default_branch).await?;
        (branch, wt)
    };

    // Copy plan briefs into worktree if they exist.
    copy_plan_briefs(item, &wt_path);

    // Run pre_spawn hook.
    let hook_env = HashMap::new();
    hooks::pre_spawn(&project_config.hooks, &wt_path, &hook_env).await?;

    // Render prompt from workflow template.
    let prompt =
        prepare_initial_worker_prompt(item, slot, &branch, &wt_path, project_config, workflow)?;

    // Spawn CC via mando-cc.
    let mut cc_builder = mando_cc::CcConfig::builder()
        .model(&workflow.models.worker)
        .effort(mando_cc::Effort::Max)
        .cwd(&wt_path)
        .session_id(&session_id)
        .caller("worker")
        .task_id(item.best_id())
        .worker_name(&session_name)
        .project(item.project.as_deref().unwrap_or(""));
    if let Some(ref fb) = workflow.models.fallback {
        cc_builder = cc_builder.fallback_model(fb);
    }
    let cc_config = cc_builder.build();

    let (pid, stream_path) = mando_cc::spawn_detached(&cc_config, &prompt, &session_id).await?;

    // Write meta sidecar for retrospective debugging.
    mando_cc::write_stream_meta(
        &mando_cc::SessionMeta {
            session_id: &session_id,
            caller: "worker",
            task_id: &item.best_id(),
            worker_name: &session_name,
            project: item.project.as_deref().unwrap_or(""),
            cwd: &wt_path.display().to_string(),
        },
        "running",
    );

    // Persist PID in health state so the review phase can check liveness.
    health_store::persist_worker_pid(&session_name, pid);

    // Log "running" session entry so the UI shows it immediately.
    crate::io::headless_cc::log_running_session(
        pool,
        &session_id,
        &wt_path,
        "worker",
        &session_name,
        &item.best_id(),
        false,
    )
    .await;

    tracing::info!(
        module = "spawner",
        worker = %session_name,
        pid = pid,
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
    })
}

/// Result of spawning a worker.
pub struct SpawnResult {
    pub session_name: String,
    pub session_id: String,
    pub pid: u32,
    pub branch: String,
    pub worktree: String,
    pub stream_path: PathBuf,
}

fn next_worker_slot(state_dir: &std::path::Path) -> Result<u64> {
    let counter_path = state_dir.join("worker-counter.txt");
    let current = match std::fs::read_to_string(&counter_path) {
        Ok(contents) => match contents.trim().parse::<u64>() {
            Ok(n) => n,
            Err(e) => {
                tracing::warn!(
                    module = "spawner",
                    path = %counter_path.display(),
                    contents = %contents.trim(),
                    error = %e,
                    "corrupt worker counter file — resetting to 0"
                );
                0
            }
        },
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
    item.linear_id
        .as_deref()
        .map(|lid| {
            let clean_lid = sanitize_linear_id(lid);
            let title_slug = mando_config::slugify(&item.title, 30);
            format!("{}-{}-{}", clean_lid.to_lowercase(), title_slug, slot)
        })
        .unwrap_or_else(|| format!("todo-{}", slot))
}

/// Sanitize a potentially corrupted `linear_id` and return just the identifier (e.g. "ENG-42").
pub(crate) fn sanitize_linear_id(lid: &str) -> String {
    let parsed = super::linear_integration::parse_issue_id(lid);
    if parsed != lid {
        tracing::warn!(module = "spawner", raw = %lid, clean = %parsed, "sanitized corrupted linear_id");
    }
    parsed
}

/// Copy plan/brief files from `~/.mando/plans/` into the worktree's `.ai/briefs/`.
///
/// Copies any files related to the item's Linear ID or plan path. Only copies
/// files that don't already exist in the destination (idempotent).
fn copy_plan_briefs(item: &Task, wt_path: &std::path::Path) {
    let plans_dir = mando_config::state_dir().join("plans");
    if !plans_dir.exists() {
        return;
    }

    let briefs_dir = wt_path.join(".ai").join("briefs");

    // Strategy 1: If item has a linear_id, look for a folder named after it.
    if let Some(ref lid) = item.linear_id {
        let clean_lid = sanitize_linear_id(lid);
        let candidate_folders = [
            plans_dir.join(&clean_lid),
            plans_dir.join(clean_lid.to_lowercase()),
        ];
        for plan_folder in candidate_folders {
            if !plan_folder.is_dir() {
                continue;
            }
            if let Ok(entries) = std::fs::read_dir(&plan_folder) {
                if let Err(e) = std::fs::create_dir_all(&briefs_dir) {
                    tracing::error!(
                        module = "spawner",
                        path = %briefs_dir.display(),
                        error = %e,
                        "failed to create briefs directory — worker will start without plan files"
                    );
                    return;
                }
                for entry in entries.flatten() {
                    let src = entry.path();
                    if src.is_file() {
                        let filename = entry.file_name();
                        let dst = briefs_dir.join(&filename);
                        if !dst.exists() {
                            if let Err(e) = std::fs::copy(&src, &dst) {
                                tracing::error!(
                                    module = "spawner",
                                    src = %src.display(),
                                    dst = %dst.display(),
                                    error = %e,
                                    "failed to copy plan brief — worker may start without this file"
                                );
                            } else {
                                tracing::info!(
                                    module = "spawner",
                                    file = %filename.to_string_lossy(),
                                    "copied plan brief to worktree"
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    // Strategy 2: Look for a generic brief file matching the item ID.
    {
        let id = &item.id.to_string();
        let brief_file = plans_dir.join(format!("item-{id}.md"));
        if brief_file.is_file() {
            if let Err(e) = std::fs::create_dir_all(&briefs_dir) {
                tracing::error!(
                    module = "spawner",
                    path = %briefs_dir.display(),
                    error = %e,
                    "failed to create briefs directory"
                );
                return;
            }
            let dst = briefs_dir.join(format!("item-{id}.md"));
            if !dst.exists() {
                if let Err(e) = std::fs::copy(&brief_file, &dst) {
                    tracing::error!(
                        module = "spawner",
                        src = %brief_file.display(),
                        dst = %dst.display(),
                        error = %e,
                        "failed to copy item brief"
                    );
                }
            }
        }
    }
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
    let clean_linear_id = item
        .linear_id
        .as_deref()
        .map(sanitize_linear_id)
        .unwrap_or_default();
    let no_pr = if item.no_pr { "true" } else { "" };

    let check_command = check_command_or_fallback(project_config);

    let mut brief_vars = HashMap::new();
    brief_vars.insert("title", item.title.as_str());
    brief_vars.insert("context", context);
    brief_vars.insert("branch", branch);
    brief_vars.insert("linear_id", clean_linear_id.as_str());
    brief_vars.insert("original_prompt", original_prompt);
    brief_vars.insert("worker_preamble", project_config.worker_preamble.as_str());
    brief_vars.insert("check_command", check_command.as_str());
    brief_vars.insert("no_pr", no_pr);
    if let Some(ref plan_path) = plan {
        brief_vars.insert("plan", plan_path.as_str());
    }

    let rendered_brief = mando_config::render_prompt(prompt_name, &workflow.prompts, &brief_vars)
        .map_err(anyhow::Error::msg)?;

    let brief_filename = worker_brief_filename(item, slot);
    let briefs_dir = wt_path.join(".ai").join("briefs");
    std::fs::create_dir_all(&briefs_dir)?;
    std::fs::write(briefs_dir.join(&brief_filename), rendered_brief)?;

    let mut vars = HashMap::new();
    vars.insert("brief_filename", brief_filename.as_str());
    vars.insert("linear_id", clean_linear_id.as_str());
    vars.insert("no_pr", no_pr);

    mando_config::render_initial_prompt(initial_prompt_name, &workflow.initial_prompts, &vars)
        .map_err(anyhow::Error::msg)
}

fn worker_brief_filename(item: &Task, slot: u64) -> String {
    if let Some(raw_lid) = item.linear_id.as_deref() {
        let sanitized = sanitize_linear_id(raw_lid);
        if sanitized.is_empty() {
            tracing::warn!(
                module = "spawner",
                raw_linear_id = %raw_lid,
                "linear_id sanitized to empty — using generic brief filename"
            );
        } else {
            return format!("{}-{slot}.md", sanitized.to_lowercase());
        }
    }
    format!("todo-{slot}.md")
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
        return Ok(Some(format!(".ai/briefs/{}", file_name.to_string_lossy())));
    }

    let relative = plan_path.trim().to_string();
    if relative.is_empty() {
        return Ok(None);
    }

    Ok(Some(relative))
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

        assert!(initial.contains(".ai/briefs/todo-1.md"));
        let brief = std::fs::read_to_string(wt.join(".ai/briefs/todo-1.md")).unwrap();
        assert!(brief.contains("Captain Brief"));
        assert!(brief.contains("Auth redirect loop in login callback"));
    }

    #[test]
    fn planned_items_render_worker_briefed_flow() {
        let wt = temp_worktree();
        let plan_source = wt.join("source-brief.md");
        std::fs::write(&plan_source, "# Brief").unwrap();

        let mut item = Task::new("Implement planned change");
        item.linear_id = Some("ABR-123".into());
        item.plan = Some(plan_source.display().to_string());

        let workflow = CaptainWorkflow::compiled_default();
        let project = ProjectConfig::default();

        prepare_initial_worker_prompt(&item, 2, "mando/abr-123", &wt, &project, &workflow).unwrap();

        let brief = std::fs::read_to_string(wt.join(".ai/briefs/abr-123-2.md")).unwrap();
        assert!(brief.contains("Human-Curated Plan"));
        assert!(brief.contains("source-brief.md"));
        assert!(wt.join(".ai/briefs/source-brief.md").exists());
    }

    #[test]
    fn tilde_plan_paths_are_copied_into_worktree_briefs() {
        let wt = temp_worktree();
        let home = std::path::PathBuf::from(std::env::var("HOME").unwrap());
        let plan_source = home.join(format!(
            ".mando/plans/ABR-tilde-test-{}/brief.md",
            mando_uuid::Uuid::v4()
        ));
        std::fs::create_dir_all(plan_source.parent().unwrap()).unwrap();
        std::fs::write(&plan_source, "# Brief from tilde path").unwrap();

        let mut item = Task::new("Implement planned change");
        item.linear_id = Some("ABR-999".into());
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

        prepare_initial_worker_prompt(&item, 4, "mando/abr-999", &wt, &project, &workflow).unwrap();

        let brief = std::fs::read_to_string(wt.join(".ai/briefs/abr-999-4.md")).unwrap();
        assert!(brief.contains(".ai/briefs/brief.md"));
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
        let brief = std::fs::read_to_string(wt.join(".ai/briefs/todo-3.md")).unwrap();
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
