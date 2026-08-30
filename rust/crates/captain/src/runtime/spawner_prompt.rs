//! Worker prompt/brief rendering helpers — pulled out of `spawner.rs`
//! so the main spawn orchestrator stays under the 500-line limit.

use anyhow::{Context, Result};
use rustc_hash::FxHashMap;
use settings::{CaptainWorkflow, ProjectConfig};

use crate::Task;

/// Template-bool convention: `"true"` is truthy, `""` is falsy. Jinja
/// coercion happens in `settings::render_template`.
fn flag(value: bool) -> &'static str {
    if value {
        "true"
    } else {
        ""
    }
}

pub(crate) fn prepare_initial_worker_prompt(
    item: &Task,
    slot: u64,
    branch: &str,
    wt_path: &std::path::Path,
    project_config: &ProjectConfig,
    workflow: &CaptainWorkflow,
) -> Result<String> {
    let plan = resolve_worker_plan_path(item, wt_path)?;
    let is_handoff = is_adopted_handoff(item, plan.as_deref(), wt_path);
    let initial_prompt_name = if is_handoff { "adopted" } else { "worker" };

    let context = item.context.as_deref().unwrap_or("");
    let original_prompt = item.original_prompt.as_deref().unwrap_or("");
    let task_id_str = item.id.to_string();
    let no_pr = flag(item.no_pr);
    let workpad_path = ensure_workpad_path(item)?;

    let mut brief_vars: FxHashMap<&str, String> = FxHashMap::default();
    brief_vars.insert("title", item.title.clone());
    brief_vars.insert("context", context.to_string());
    brief_vars.insert("branch", branch.to_string());
    brief_vars.insert("id", task_id_str);
    brief_vars.insert("original_prompt", original_prompt.to_string());
    brief_vars.insert("worker_preamble", project_config.worker_preamble.clone());
    brief_vars.insert("no_pr", no_pr.to_string());
    brief_vars.insert("is_bug_fix", flag(item.is_bug_fix).to_string());
    brief_vars.insert("workpad_path", workpad_path.clone());
    brief_vars.insert("plan", plan.unwrap_or_default());
    brief_vars.insert("is_handoff", flag(is_handoff).to_string());

    let rendered_brief = settings::render_prompt("worker", &workflow.prompts, &brief_vars)
        .map_err(anyhow::Error::msg)?;

    let brief_filename = worker_brief_filename(item, slot);
    let briefs_dir = wt_path.join(".ai").join("briefs");
    std::fs::create_dir_all(&briefs_dir)?;
    let brief_path = briefs_dir.join(&brief_filename);
    std::fs::write(&brief_path, rendered_brief)?;

    let mut vars: FxHashMap<&str, String> = FxHashMap::default();
    vars.insert("brief_path", brief_path.display().to_string());
    vars.insert("no_pr", no_pr.to_string());
    vars.insert("workpad_path", workpad_path);

    settings::render_initial_prompt(initial_prompt_name, &workflow.initial_prompts, &vars)
        .map_err(anyhow::Error::msg)
}

fn ensure_workpad_path(item: &Task) -> Result<String> {
    let workpad_path = global_infra::paths::data_dir()
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

    let plan = global_infra::paths::expand_tilde(plan_path);
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

pub(crate) fn is_adopted_handoff(
    item: &Task,
    plan: Option<&str>,
    wt_path: &std::path::Path,
) -> bool {
    plan.is_some_and(|path| path.ends_with("adopt-handoff.md"))
        && item.worktree.is_some()
        && item.branch.is_some()
        && wt_path.exists()
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

    /// The unified `worker` prompt: no plan, no handoff, PR expected.
    #[test]
    fn generic_items_render_the_worker_brief() {
        let wt = temp_worktree();
        let mut item = Task::new("Fix auth redirect");
        item.context = Some("Auth redirect loop in login callback".into());
        let workflow = CaptainWorkflow::compiled_default();
        let project = ProjectConfig::default();

        let initial =
            prepare_initial_worker_prompt(&item, 1, "mando/fix-auth-1", &wt, &project, &workflow)
                .unwrap();

        let workpad = global_infra::paths::data_dir()
            .join("plans/0/workpad.md")
            .display()
            .to_string();
        assert!(initial.contains(&wt.join(".ai/briefs/todo-0-1.md").display().to_string()));
        assert!(initial.contains(&workpad));
        // no_pr = false, so the initial prompt keeps the /x-pr handoff line.
        assert!(initial.contains("/x-pr"));

        let brief = std::fs::read_to_string(wt.join(".ai/briefs/todo-0-1.md")).unwrap();
        assert!(brief.contains("Captain Brief"));
        assert!(brief.contains("Auth redirect loop in login callback"));
        assert!(brief.contains(&workpad));
        assert!(brief.contains("## Evidence Deck"));
        assert!(brief.contains("## Out-of-Scope Discoveries"));
        // Neither optional branch fires without a plan or a handoff.
        assert!(!brief.contains("## Brief"));
        assert!(!brief.contains("## Handoff"));
    }

    /// A human-authored brief (`mando todo add --plan <path>`) lands on
    /// `task.plan`, is copied into the worktree, and reaches the worker
    /// prompt's plan branch.
    #[test]
    fn plan_path_reaches_the_worker_prompts_plan_branch() {
        let wt = temp_worktree();
        let plan_source = wt.join("source-brief.md");
        std::fs::write(&plan_source, "# Brief").unwrap();

        let mut item = Task::new("Implement planned change");
        item.plan = Some(plan_source.display().to_string());

        let workflow = CaptainWorkflow::compiled_default();
        let project = ProjectConfig::default();

        prepare_initial_worker_prompt(&item, 2, "mando/todo-0", &wt, &project, &workflow).unwrap();

        let brief = std::fs::read_to_string(wt.join(".ai/briefs/todo-0-2.md")).unwrap();
        let copied = wt.join(".ai/briefs/source-brief.md");
        assert!(brief.contains("## Brief"), "plan branch missing: {brief}");
        assert!(brief.contains("Read the plan file first"));
        assert!(brief.contains(&copied.display().to_string()));
        assert!(copied.exists());
        assert!(!brief.contains("## Handoff"));
    }

    #[test]
    fn tilde_plan_paths_are_copied_into_worktree_briefs() {
        let wt = temp_worktree();
        let home = std::path::PathBuf::from(std::env::var("HOME").unwrap());
        let plan_source = home.join(format!(
            ".mando/plans/tilde-test-{}/brief.md",
            global_infra::uuid::Uuid::v4()
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
    fn adopted_items_render_the_handoff_branch() {
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

        // The `adopted` initial prompt, not the plain `worker` one.
        assert!(initial.contains("handed off to you"));
        assert!(initial.contains(
            &global_infra::paths::data_dir()
                .join("plans/0/workpad.md")
                .display()
                .to_string()
        ));

        let brief = std::fs::read_to_string(wt.join(".ai/briefs/todo-0-3.md")).unwrap();
        assert!(brief.contains("## Handoff"));
        assert!(brief.contains("handed off mid-implementation"));
        assert!(brief.contains("## Out-of-Scope Discoveries"));
    }

    #[test]
    fn no_pr_items_drop_the_evidence_and_pr_sections() {
        let wt = temp_worktree();
        let mut item = Task::new("Audit the pricing table");
        item.no_pr = true;

        let workflow = CaptainWorkflow::compiled_default();
        let project = ProjectConfig::default();

        let initial =
            prepare_initial_worker_prompt(&item, 5, "mando/audit-0", &wt, &project, &workflow)
                .unwrap();
        assert!(
            !initial.contains("/x-pr"),
            "no-PR task must not be told to open a PR"
        );

        let brief = std::fs::read_to_string(wt.join(".ai/briefs/todo-0-5.md")).unwrap();
        assert!(!brief.contains("## Evidence Deck"));
        assert!(!brief.contains("## Finishing"));
        assert!(brief.contains("This is a research/audit task: no PR."));
    }

    #[test]
    fn bug_fix_items_render_the_reproduce_first_protocol() {
        let wt = temp_worktree();
        let mut item = Task::new("Login button overflows on mobile");
        item.is_bug_fix = true;

        let workflow = CaptainWorkflow::compiled_default();
        let project = ProjectConfig::default();

        prepare_initial_worker_prompt(&item, 6, "mando/bug-0", &wt, &project, &workflow).unwrap();

        let brief = std::fs::read_to_string(wt.join(".ai/briefs/todo-0-6.md")).unwrap();
        assert!(brief.contains("## Bug Fix Protocol"));
        assert!(brief.contains("Reproduce the bug before changing code."));
    }

    #[test]
    fn adopt_requires_all_four_conditions() {
        let wt = temp_worktree();

        let mut item = Task::new("test");
        item.worktree = Some(wt.display().to_string());
        item.branch = Some("b".into());
        assert!(is_adopted_handoff(
            &item,
            Some(".ai/briefs/adopt-handoff.md"),
            &wt
        ));

        let mut item2 = Task::new("test");
        item2.branch = Some("b".into());
        assert!(!is_adopted_handoff(
            &item2,
            Some(".ai/briefs/adopt-handoff.md"),
            &wt
        ));

        let mut item3 = Task::new("test");
        item3.worktree = Some(wt.display().to_string());
        assert!(!is_adopted_handoff(
            &item3,
            Some(".ai/briefs/adopt-handoff.md"),
            &wt
        ));

        let mut item4 = Task::new("test");
        item4.worktree = Some(wt.display().to_string());
        item4.branch = Some("b".into());
        assert!(!is_adopted_handoff(
            &item4,
            Some(".ai/briefs/regular-brief.md"),
            &wt
        ));

        let mut item5 = Task::new("test");
        item5.worktree = Some(wt.display().to_string());
        item5.branch = Some("b".into());
        assert!(!is_adopted_handoff(&item5, None, &wt));

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
