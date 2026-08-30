//! Spawner lifecycle — restart, rework, reopen worker orchestration.

use std::path::Path;

use crate::Task;
use anyhow::{Context, Result};
use rustc_hash::FxHashMap;
use settings::CaptainWorkflow;
use settings::{Config, ProjectConfig};

/// Result of a lifecycle operation (restart/rework/reopen).
pub struct LifecycleResult {
    pub session_name: String,
    pub session_id: String,
    pub branch: String,
    pub worktree: String,
    /// Post-op absolute state: the PR the task is on *after* this call.
    /// Resume carries the existing PR through; fresh-spawn surfaces the
    /// freshly-created one, or `None` when `create_draft_pr` failed. Caller
    /// overwrites `item.pr_number` with this value unconditionally.
    pub pr_number: Option<i64>,
}

/// Reset the worktree in place and spawn a fresh worker.
///
/// Invoked when reopen detects a broken CC session (no init event in the
/// stream). Captain invariant #4 in CLAUDE.md: a task's worktree is
/// permanent once assigned, so we do NOT delete the worktree dir. Clearing
/// `item.branch` steers the spawner's 3-way match (`spawner.rs:51-75`)
/// into its Rework arm, which does `git reset --hard && git clean -fd &&
/// git checkout -B <new_branch> origin/main` to wipe broken-session state
/// in place without losing the worktree binding.
async fn clean_and_spawn_fresh(
    item: &mut Task,
    project_config: &ProjectConfig,
    workflow: &CaptainWorkflow,
    pool: &sqlx::SqlitePool,
) -> Result<LifecycleResult> {
    item.branch = None;
    item.worker_seq += 1;
    let result = super::agent_runtime::spawn_worker(project_config, item, workflow, pool).await?;
    Ok(LifecycleResult {
        session_name: result.session_name,
        session_id: result.session_id,
        branch: result.branch,
        worktree: result.worktree,
        pr_number: result.pr_number,
    })
}

/// Reopen a worker — kill and respawn with review feedback context.
#[tracing::instrument(skip_all)]
pub(crate) async fn reopen_worker(
    item: &mut Task,
    config: &Config,
    feedback: &str,
    workflow: &CaptainWorkflow,
    pool: &sqlx::SqlitePool,
) -> Result<LifecycleResult> {
    let (_, project_config) = resolve_project(item, config)?;
    let wt_path = item
        .worktree
        .clone()
        .ok_or_else(|| anyhow::anyhow!("no worktree for reopen"))?;
    let session_name = item
        .worker
        .clone()
        .ok_or_else(|| anyhow::anyhow!("no worker name for reopen"))?;
    let cc_sid = item
        .session_ids
        .worker
        .clone()
        .ok_or_else(|| anyhow::anyhow!("no cc_session_id for reopen"))?;
    // Read branch live from the worktree — item.branch is not persisted in the
    // DB and is only populated during captain ticks by tick_branch_sync.  HTTP
    // handlers load the task fresh from DB where branch is always None.
    let wt_expanded = global_infra::paths::expand_tilde(&wt_path);
    let branch = global_git::current_branch(&wt_expanded)
        .await
        .with_context(|| format!("failed to read branch from worktree {}", wt_path))?;
    if branch == "HEAD" {
        anyhow::bail!(
            "worktree {} is in detached HEAD state — cannot reopen",
            wt_path
        );
    }
    let fallback_provider = super::agent_runtime::implementation_provider(item, workflow);
    let worker_provider =
        super::agent_runtime::persisted_session_provider(pool, &cc_sid, fallback_provider).await;

    // Stop any existing provider-owned worker process before resuming.
    if let Err(e) = super::agent_runtime::terminate_worker_process(worker_provider, &cc_sid).await {
        tracing::warn!(
            module = "captain",
            provider = %worker_provider.as_str(),
            session_id = %cc_sid,
            error = %e,
            "failed to terminate existing worker for reopen"
        );
    }

    // Write reopen context (include image paths if any are attached).
    let reopen_seq = item.reopen_seq + 1;
    let images_section = attached_image_lines(item.images.as_deref());

    let reopen_seq_str = reopen_seq.to_string();
    let mut vars: FxHashMap<&str, &str> = FxHashMap::default();
    vars.insert("reopen_seq", reopen_seq_str.as_str());
    vars.insert("feedback", feedback);
    vars.insert("images", images_section.as_str());
    let reopen_context = settings::render_prompt("reopen_context", &workflow.prompts, &vars)
        .map_err(|e| anyhow::anyhow!(e))?;
    write_context_file(&wt_expanded, "captain-reopen-context.md", &reopen_context).await?;

    vars.remove("feedback");
    vars.remove("images");
    let resume_msg = settings::render_prompt("reopen_resume", &workflow.prompts, &vars)
        .map_err(|e| anyhow::anyhow!(e))?;

    // Record stream file size before resume for zero-byte detection.
    let stream_path = super::agent_session_result::stream_path(worker_provider, &cc_sid);
    let stream_size_before = std::fs::metadata(&stream_path)
        .map(|m| m.len())
        .unwrap_or(0);

    if let Some(reason) = super::agent_runtime::worker_resume_replacement_reason(
        worker_provider,
        &stream_path,
        workflow,
    ) {
        tracing::warn!(
            module = "lifecycle",
            worker = %session_name,
            provider = %worker_provider,
            cc_sid,
            reason = %reason,
            "resume session is not reusable — spawning fresh"
        );
        return clean_and_spawn_fresh(item, project_config, workflow, pool).await;
    }

    match super::agent_runtime::resume_worker(
        pool,
        item,
        &session_name,
        &wt_expanded,
        &resume_msg,
        &cc_sid,
        &workflow.models.worker,
        workflow,
    )
    .await
    {
        Ok(resume) => {
            let health_path = crate::config::worker_health_path();
            let mut state = crate::io::health_store::load_health_state_async(&health_path)
                .await
                .with_context(|| format!("load health state from {}", health_path.display()))?;
            crate::io::health_store::set_health_field(
                &mut state,
                &session_name,
                "pid",
                serde_json::json!(resume.pid),
            );
            crate::io::health_store::set_health_field(
                &mut state,
                &session_name,
                "stream_size_at_spawn",
                serde_json::json!(stream_size_before),
            );
            if let Err(e) = crate::io::health_store::save_health_state(&health_path, &state) {
                tracing::error!(
                    module = "captain",
                    worker = %session_name,
                    error = %e,
                    "failed to persist health state; zero-byte resume detection may be disabled"
                );
            }
            tracing::info!(
                module = "lifecycle",
                worker = %session_name,
                pid = %resume.pid,
                reopen_seq = reopen_seq,
                title = %item.title,
                "reopened worker"
            );
            Ok(LifecycleResult {
                session_name,
                session_id: resume.session_id,
                branch,
                worktree: wt_path,
                // Resume preserves the existing PR — no new branch, no new
                // `create_draft_pr`. `reopen_worker` does not mutate
                // `item.pr_number` before this point, so re-capturing it
                // here matches the task's on-disk PR.
                pr_number: item.pr_number,
            })
        }
        Err(e) => {
            // Do NOT silently destroy the worktree by falling through to
            // clean_and_spawn_fresh. Resume failure could be transient; the
            // caller must explicitly opt into a fresh spawn (e.g. by calling
            // rework instead of reopen).
            tracing::warn!(
                module = "lifecycle",
                worker = %session_name,
                error = %e,
                "reopen resume failed; refusing to auto-destroy worktree"
            );
            Err(anyhow::anyhow!(
                "reopen resume failed for worker {session_name}: {e}"
            ))
        }
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn resolve_project<'a>(item: &Task, config: &'a Config) -> Result<(&'a str, &'a ProjectConfig)> {
    settings::resolve_project_config(Some(&item.project), config)
        .ok_or_else(|| anyhow::anyhow!("no project config for item '{}'", item.title))
}

async fn write_context_file(worktree: &Path, filename: &str, content: &str) -> Result<()> {
    let ai_dir = worktree.join(".ai");
    tokio::fs::create_dir_all(&ai_dir).await?;
    tokio::fs::write(ai_dir.join(filename), content).await?;
    Ok(())
}

/// Absolute paths for the images attached to a task, one markdown bullet per
/// line, for the reopen-context template. Empty when nothing is attached.
/// Basenames only: a stored value with a directory component or `..` is
/// dropped rather than resolved outside the images dir.
fn attached_image_lines(images: Option<&str>) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attached_image_lines_is_empty_without_images() {
        assert_eq!(attached_image_lines(None), "");
        assert_eq!(attached_image_lines(Some("")), "");
    }

    #[test]
    fn attached_image_lines_expands_basenames_to_the_images_dir() {
        let dir = global_infra::paths::images_dir();
        let rendered = attached_image_lines(Some("a.png, b.png"));
        assert_eq!(
            rendered,
            format!(
                "- {}\n- {}",
                dir.join("a.png").display(),
                dir.join("b.png").display()
            )
        );
    }

    #[test]
    fn attached_image_lines_drops_path_traversal_and_directories() {
        assert_eq!(attached_image_lines(Some("../secret.png")), "");
        assert_eq!(attached_image_lines(Some("nested/a.png")), "");
    }
}
