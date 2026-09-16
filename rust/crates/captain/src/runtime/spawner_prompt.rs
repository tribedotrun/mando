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
    brief_vars.insert("images", attached_image_lines(item.images.as_deref()));
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

/// Absolute paths for the images attached to a task, one markdown bullet per
/// line, for initial, clarifier, and reopen prompts. Empty when nothing is attached.
/// Basenames only: a stored value with a directory component or `..` is
/// dropped rather than resolved outside the images dir.
pub(crate) fn attached_image_lines(images: Option<&str>) -> String {
    let Some(images) = images.filter(|s| !s.is_empty()) else {
        return String::new();
    };
    let dir = global_infra::paths::images_dir();
    images
        .split(',')
        .filter_map(|entry| {
            let name = entry.trim();
            let base = std::path::Path::new(name).file_name()?.to_str()?;
            (base == name && !name.contains(".."))
                .then(|| format!("- {}", dir.join(base).display()))
        })
        .collect::<Vec<_>>()
        .join("\n")
}
