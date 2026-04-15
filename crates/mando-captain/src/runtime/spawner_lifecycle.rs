//! Spawner lifecycle — restart, rework, reopen worker orchestration.

use std::path::Path;

use anyhow::{Context, Result};
use mando_config::settings::{Config, ProjectConfig};
use mando_config::workflow::CaptainWorkflow;
use mando_types::Task;
use rustc_hash::FxHashMap;

use crate::io::{git, pid_registry, process_manager};
use crate::runtime::spawner;

/// Result of a lifecycle operation (restart/rework/reopen).
pub struct LifecycleResult {
    pub session_name: String,
    pub session_id: String,
    pub pid: mando_types::Pid,
    pub branch: String,
    pub worktree: String,
}

/// Clean old worktree and spawn a completely fresh worker.
async fn clean_and_spawn_fresh(
    item: &mut Task,
    slug: &str,
    project_config: &ProjectConfig,
    config: &Config,
    workflow: &CaptainWorkflow,
    wt_path: &str,
    pool: &sqlx::SqlitePool,
) -> Result<LifecycleResult> {
    let old_wt = mando_config::expand_tilde(wt_path);
    if tokio::fs::try_exists(&old_wt).await.unwrap_or(false) {
        let repo_path = mando_config::expand_tilde(&project_config.path);
        match git::remove_worktree(&repo_path, &old_wt).await {
            Ok(_) => {
                tracing::info!(module = "lifecycle", path = %old_wt.display(), "cleaned old worktree")
            }
            Err(e) => {
                tracing::warn!(module = "lifecycle", path = %old_wt.display(), error = %e, "failed to clean old worktree")
            }
        }
    }
    item.worker_seq += 1;
    // Pick credential for the reworked worker. Balance on worker sessions only.
    let credential = super::tick_spawn::pick_credential(pool, Some("worker")).await;
    let worker_cred = credential.as_ref().map(|c| spawner::WorkerCredential {
        id: c.0,
        token: &c.1,
    });
    let result = spawner::spawn_worker(
        item,
        slug,
        project_config,
        &config.captain,
        workflow,
        pool,
        worker_cred.as_ref(),
    )
    .await?;
    Ok(LifecycleResult {
        session_name: result.session_name,
        session_id: result.session_id,
        pid: result.pid,
        branch: result.branch,
        worktree: result.worktree,
    })
}

/// Reopen a worker — kill and respawn with review feedback context.
pub(crate) async fn reopen_worker(
    item: &mut Task,
    config: &Config,
    feedback: &str,
    workflow: &CaptainWorkflow,
    pool: &sqlx::SqlitePool,
) -> Result<LifecycleResult> {
    let (slug, project_config) = resolve_project(item, config)?;
    let slug = slug.to_string();
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
    let wt_expanded = mando_config::expand_tilde(&wt_path);
    let branch = git::current_branch(&wt_expanded)
        .await
        .with_context(|| format!("failed to read branch from worktree {}", wt_path))?;
    if branch == "HEAD" {
        anyhow::bail!(
            "worktree {} is in detached HEAD state — cannot reopen",
            wt_path
        );
    }

    // Kill existing worker (fingerprint-verified to avoid PID reuse).
    let pid = pid_registry::get_verified_pid(&cc_sid).unwrap_or(mando_types::Pid::new(0));
    if pid.as_u32() > 0 {
        if let Err(e) = mando_cc::kill_process(pid).await {
            tracing::warn!(module = "captain", pid = %pid, error = %e, "failed to kill existing worker for reopen");
        }
    }

    // Write reopen context (include image paths if any are attached).
    let reopen_seq = item.reopen_seq + 1;
    let images_section = item
        .images
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(|s| {
            let dir = mando_config::images_dir();
            let lines: Vec<String> = s
                .split(',')
                .filter_map(|f| {
                    let name = f.trim();
                    let base = std::path::Path::new(name).file_name()?.to_str()?;
                    (base == name && !name.contains(".."))
                        .then(|| format!("- {}", dir.join(base).display()))
                })
                .collect();
            if lines.is_empty() {
                String::new()
            } else {
                format!("\n\n## Attached Images\n{}\n", lines.join("\n"))
            }
        })
        .unwrap_or_default();
    write_context_file(
        &wt_expanded,
        "captain-reopen-context.md",
        &format!(
            "# Captain Reopen (seq={})\n\nReview feedback:\n{}\n\nAddress the feedback, then post an ack comment: `[Mando] Reopen #{} addressed: <summary>`\n\nIf you make code changes, recapture and update PR evidence.\n{}",
            reopen_seq, feedback, reopen_seq, images_section
        ),
    )
    .await?;

    // Check if the session was ever created — broken session guard.
    let stream_path = mando_config::stream_path_for_session(&cc_sid);
    if mando_cc::stream_has_broken_session(&stream_path) {
        tracing::warn!(
            module = "lifecycle",
            worker = %session_name,
            cc_sid,
            "no init event in stream — session was never created, spawning fresh"
        );
        return clean_and_spawn_fresh(
            item,
            &slug,
            project_config,
            config,
            workflow,
            &wt_path,
            pool,
        )
        .await;
    }

    let model = &workflow.models.worker;
    let (mut env, reopen_cred_id) = super::spawner::credential_env_for_session(pool, &cc_sid).await;
    env.insert("MANDO_TASK_ID".to_string(), item.id.to_string());

    let reopen_seq_str = reopen_seq.to_string();
    let mut vars: FxHashMap<&str, &str> = FxHashMap::default();
    vars.insert("reopen_seq", reopen_seq_str.as_str());
    let resume_msg = mando_config::render_prompt("reopen_resume", &workflow.prompts, &vars)
        .map_err(|e| anyhow::anyhow!(e))?;

    let item_id = Some(item.id);

    // Record stream file size before resume for zero-byte detection.
    let stream_size_before = mando_cc::get_stream_file_size(&stream_path);

    match process_manager::resume_worker_process(
        &resume_msg,
        &wt_expanded,
        model,
        &cc_sid,
        &env,
        workflow.models.fallback.as_deref(),
    )
    .await
    {
        Ok((new_pid, _)) => {
            // Register PID in the session registry.
            pid_registry::register(&cc_sid, new_pid)?;

            let health_path = mando_config::worker_health_path();
            let mut state = crate::io::health_store::load_health_state_async(&health_path)
                .await
                .with_context(|| format!("load health state from {}", health_path.display()))?;
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
            crate::io::headless_cc::log_running_session(
                pool,
                &cc_sid,
                &wt_expanded,
                "worker",
                &session_name,
                item_id,
                true,
                reopen_cred_id,
            )
            .await?;
            tracing::info!(
                module = "lifecycle",
                worker = %session_name,
                pid = %new_pid,
                reopen_seq = reopen_seq,
                title = %item.title,
                "reopened worker"
            );
            Ok(LifecycleResult {
                session_name,
                session_id: cc_sid,
                pid: new_pid,
                branch,
                worktree: wt_path,
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
    mando_config::resolve_project_config(Some(&item.project), config)
        .ok_or_else(|| anyhow::anyhow!("no project config for item '{}'", item.title))
}

async fn write_context_file(worktree: &Path, filename: &str, content: &str) -> Result<()> {
    let ai_dir = worktree.join(".ai");
    tokio::fs::create_dir_all(&ai_dir).await?;
    tokio::fs::write(ai_dir.join(filename), content).await?;
    Ok(())
}
