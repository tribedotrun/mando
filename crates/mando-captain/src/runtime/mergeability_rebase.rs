//! Rebase worker management and PR status checking — extracted from mergeability.

use anyhow::Result;
use mando_config::settings::Config;
use mando_config::workflow::CaptainWorkflow;
use mando_types::Task;

use crate::biz::merge_logic;
use crate::runtime::notify::Notifier;

pub(super) enum MergeStatus {
    Merged,
    Closed,
    Mergeable,
    Conflicted,
    Unknown,
}

/// Resolve the GitHub repo slug from a project name via config lookup.
pub(super) fn resolve_github_repo(project_name: Option<&str>, config: &Config) -> Option<String> {
    mando_config::resolve_github_repo(project_name, config)
}

/// Check PR mergeable status via `gh pr view`.
pub(super) async fn check_pr_mergeable(pr: &str, repo: &str) -> Result<MergeStatus> {
    let pr_num = pr.trim_start_matches('#');
    let mut cmd = tokio::process::Command::new("gh");
    cmd.args([
        "pr",
        "view",
        pr_num,
        "--json",
        "state,mergeable,mergeStateStatus",
    ]);
    if !repo.is_empty() {
        cmd.args(["--repo", repo]);
    }

    let output = cmd.output().await?;
    if !output.status.success() {
        anyhow::bail!(
            "gh pr view failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let json: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let state = json["state"].as_str().unwrap_or("");
    let mergeable = json["mergeable"].as_str().unwrap_or("");

    match state {
        "MERGED" => Ok(MergeStatus::Merged),
        "CLOSED" => Ok(MergeStatus::Closed),
        _ => match mergeable {
            "MERGEABLE" => Ok(MergeStatus::Mergeable),
            "CONFLICTING" => Ok(MergeStatus::Conflicted),
            _ => Ok(MergeStatus::Unknown),
        },
    }
}

/// Reap dead rebase workers and detect success via SHA comparison.
///
/// Uses pid_registry for PID lookup.
pub(super) async fn reap_dead_rebase_workers(items: &mut [Task]) {
    for item in items.iter_mut() {
        let rw = match &item.rebase_worker {
            Some(rw) if rw != "failed" => rw.clone(),
            _ => continue,
        };
        // Look up rebase worker PID from pid_registry (registered by session_name).
        let pid = crate::io::pid_registry::get_pid(&rw).unwrap_or(0);
        if pid != 0 && mando_cc::is_process_alive(pid) {
            continue; // still running
        }

        // Worker exited. Check if it succeeded by comparing HEAD SHA.
        let wt = item.worktree.as_deref().unwrap_or("");
        let succeeded = if !wt.is_empty() {
            let wt_path = mando_config::expand_tilde(wt);
            match crate::io::git::head_sha(&wt_path).await {
                Ok(current_sha) => {
                    merge_logic::did_rebase_succeed(item.rebase_head_sha.as_deref(), &current_sha)
                }
                Err(e) => {
                    tracing::warn!(
                        module = "captain",
                        worker = %rw,
                        wt = %wt,
                        error = %e,
                        "failed to read HEAD SHA for rebase success detection, treating as failure"
                    );
                    false
                }
            }
        } else {
            false
        };

        if succeeded {
            tracing::info!(
                module = "captain",
                worker = %rw,
                "rebase worker succeeded (SHA changed), resetting retries"
            );
            item.rebase_retries = 0;
            item.rebase_head_sha = None;
        } else {
            tracing::info!(
                module = "captain",
                worker = %rw,
                retries = item.rebase_retries,
                "rebase worker failed (SHA unchanged)"
            );
        }

        // Unregister PID for the worker_name key (registered at spawn time).
        crate::io::pid_registry::unregister(&rw);
        item.rebase_worker = None;
    }
}

/// Handle a conflicted PR — spawn a rebase worker or declare exhaustion.
#[allow(clippy::too_many_arguments)]
pub(super) async fn handle_conflict(
    items: &mut [Task],
    idx: usize,
    pr: &str,
    config: &Config,
    workflow: &CaptainWorkflow,
    notifier: &Notifier,
    alerts: &mut Vec<String>,
    pool: &sqlx::SqlitePool,
) {
    let item = &items[idx];
    let rebase_retries = item.rebase_retries as u32;
    let max_rebase_retries = workflow.agent.max_rebase_retries;

    if rebase_retries >= max_rebase_retries {
        let item = &mut items[idx];
        item.rebase_worker = Some("failed".into());
        let msg = format!(
            "\u{274c} Rebase failed (PR {}, {} retries): <b>{}</b> — needs manual intervention",
            pr,
            max_rebase_retries,
            mando_shared::telegram_format::escape_html(&item.title),
        );
        alerts.push(msg.clone());
        notifier.critical(&msg).await;
        super::timeline_emit::emit_for_task(
            item,
            mando_types::timeline::TimelineEventType::Escalated,
            &format!(
                "Rebase failed after {} retries (PR {})",
                max_rebase_retries, pr
            ),
            serde_json::json!({
                "pr": pr,
                "retries": max_rebase_retries,
                "reason": "rebase_exhausted",
            }),
            pool,
        )
        .await;
        return;
    }

    // Check exponential backoff: don't spawn if not enough time has passed.
    let delay_s = merge_logic::rebase_delay_s(rebase_retries, workflow.agent.rebase_base_delay_s);
    if delay_s > 0 {
        if let Some(ref last_activity) = item.last_activity_at {
            match time::OffsetDateTime::parse(
                last_activity,
                &time::format_description::well_known::Rfc3339,
            ) {
                Ok(last) => {
                    let elapsed = (time::OffsetDateTime::now_utc() - last)
                        .whole_seconds()
                        .max(0) as u64;
                    if elapsed < delay_s {
                        tracing::debug!(
                            module = "captain",
                            pr = %pr,
                            delay_s,
                            elapsed,
                            "rebase backoff — waiting"
                        );
                        return;
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        module = "captain",
                        pr = %pr,
                        last_activity = %last_activity,
                        error = %e,
                        "failed to parse last_activity_at for backoff — spawning immediately"
                    );
                }
            }
        }
    }

    // Spawn rebase worker.
    let project_name = item.project.as_deref().unwrap_or("");
    let Some((_, project_config)) =
        mando_config::resolve_project_config(Some(project_name), config)
    else {
        tracing::warn!(
            module = "captain",
            pr = %pr,
            project = project_name,
            "skipping rebase — no project config found"
        );
        return;
    };

    let repo_path = mando_config::expand_tilde(&project_config.path);
    let default_branch_raw = crate::io::git::default_branch(&repo_path)
        .await
        .unwrap_or_else(|_| "main".into());
    let default_branch = default_branch_raw
        .strip_prefix("origin/")
        .unwrap_or(&default_branch_raw)
        .to_string();
    let branch = item.branch.as_deref().unwrap_or("");
    let pr_num = mando_types::task::extract_pr_number(pr)
        .unwrap_or(pr.trim_start_matches('#'))
        .to_string();

    tracing::info!(
        module = "captain",
        pr = %pr,
        retry = rebase_retries + 1,
        "spawning rebase worker"
    );

    let rebase_tmpl = workflow
        .prompts
        .get("rebase_worker")
        .cloned()
        .unwrap_or_else(|| {
            "Rebase {{ branch }} onto origin/{{ default_branch }}. PR #{{ pr_num }}.".into()
        });
    let prompt = match mando_config::render_template(
        &rebase_tmpl,
        &[
            ("branch", branch),
            ("default_branch", &default_branch),
            ("pr_num", &pr_num),
            ("attempt", &(rebase_retries + 1).to_string()),
            ("max_retries", &max_rebase_retries.to_string()),
        ]
        .into_iter()
        .collect(),
    ) {
        Ok(p) => p,
        Err(e) => {
            tracing::error!(module = "captain", error = %e, "failed to render rebase_worker template");
            return;
        }
    };

    let wt = item.worktree.as_deref().unwrap_or("");
    let wt_path = mando_config::expand_tilde(wt);

    // Abort any stale rebase left over from a prior crashed worker.
    if !wt.is_empty() {
        let abort = tokio::process::Command::new("git")
            .args(["rebase", "--abort"])
            .current_dir(&wt_path)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .await;
        if let Ok(s) = abort {
            if s.success() {
                tracing::info!(
                    module = "captain",
                    pr = %pr,
                    "aborted stale rebase before spawning worker"
                );
            }
        }
    }

    // Record HEAD SHA *after* abort so we get the stable branch tip, not a mid-rebase commit.
    let head_sha = if !wt.is_empty() {
        match crate::io::git::head_sha(&wt_path).await {
            Ok(sha) => Some(sha),
            Err(e) => {
                tracing::warn!(
                    module = "captain",
                    pr = %pr,
                    error = %e,
                    "failed to record baseline HEAD SHA — success detection disabled for this attempt"
                );
                None
            }
        }
    } else {
        None
    };

    let session_name = format!("mando-rebase-{}", pr_num);
    let session_id = mando_uuid::Uuid::v4().to_string();

    match crate::io::process_manager::spawn_worker_process(
        &session_name,
        &prompt,
        &wt_path,
        &workflow.models.worker,
        &session_id,
        &std::collections::HashMap::new(),
        workflow.models.fallback.as_deref(),
    )
    .await
    {
        Ok((pid, _)) => {
            // Register by session_name (not session_id) — reap_dead_rebase_workers
            // looks up by worker name and unregisters it on completion.
            crate::io::pid_registry::register(&session_name, pid);
            let item = &mut items[idx];
            item.rebase_worker = Some(session_name.clone());
            item.rebase_retries = merge_logic::next_rebase_retry(item) as i64;
            item.rebase_head_sha = head_sha;
            item.last_activity_at = Some(
                time::OffsetDateTime::now_utc()
                    .format(&time::format_description::well_known::Rfc3339)
                    .unwrap_or_default(),
            );
            tracing::info!(
                module = "captain",
                worker = %session_name,
                pid = pid,
                "rebase worker spawned"
            );
            super::timeline_emit::emit_for_task(
                item,
                mando_types::timeline::TimelineEventType::RebaseTriggered,
                &format!(
                    "Rebase worker {} spawned (attempt {}/{})",
                    session_name,
                    rebase_retries + 1,
                    max_rebase_retries
                ),
                serde_json::json!({
                    "worker": session_name,
                    "session_id": session_id,
                    "pr": pr,
                    "attempt": rebase_retries + 1,
                    "max_retries": max_rebase_retries,
                }),
                pool,
            )
            .await;
        }
        Err(e) => {
            alerts.push(format!("Rebase spawn failed for PR {pr}: {e}"));
        }
    }
}
