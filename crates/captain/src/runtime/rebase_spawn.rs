//! Rebase worker spawning — handles conflict detection response and worker lifecycle setup.

use crate::Task;
use settings::config::settings::Config;
use settings::config::workflow::CaptainWorkflow;

use crate::runtime::notify::Notifier;
use crate::service::merge_logic;

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
        let snap = super::action_contract::ReviewFieldsSnapshot::capture(item);
        let saved_rebase_worker = item.rebase_worker.clone();
        item.rebase_worker = Some("failed".into());
        let title = global_infra::html::escape_html(&item.title);

        // Route through CaptainReviewing (rebase_fail trigger) instead of
        // escalating directly — invariant 1: Escalated only via CaptainReviewing verdict.
        super::action_contract::reset_review_retry(item, crate::ReviewTrigger::RebaseFail);

        let event = crate::TimelineEvent {
            event_type: crate::TimelineEventType::CaptainReviewStarted,
            timestamp: global_types::now_rfc3339(),
            actor: "captain".to_string(),
            summary: format!(
                "Rebase failed after {} retries (PR {}) — captain reviewing",
                max_rebase_retries, pr
            ),
            data: serde_json::json!({
                "pr": pr,
                "retries": max_rebase_retries,
                "reason": "rebase_exhausted",
            }),
        };
        match crate::io::queries::tasks::persist_status_transition(
            pool,
            item,
            snap.status.as_str(),
            &event,
        )
        .await
        {
            Ok(true) => {
                let msg = format!(
                    "\u{274c} Rebase failed (PR {}, {} retries): <b>{}</b> — captain reviewing",
                    pr, max_rebase_retries, title,
                );
                alerts.push(msg.clone());
                notifier.critical(&msg).await;
            }
            Ok(false) => {
                tracing::info!(
                    module = "captain",
                    "rebase exhausted transition already applied"
                );
            }
            Err(e) => {
                snap.restore(item);
                item.rebase_worker = saved_rebase_worker;
                tracing::error!(module = "captain", error = %e, "persist failed for rebase exhausted");
            }
        }
        return;
    }

    // Check exponential backoff: don't spawn if not enough time has passed.
    let delay = merge_logic::rebase_delay(rebase_retries, workflow.agent.rebase_base_delay_s);
    if !delay.is_zero() {
        if let Some(ref last_activity) = item.last_activity_at {
            match time::OffsetDateTime::parse(
                last_activity,
                &time::format_description::well_known::Rfc3339,
            ) {
                Ok(last) => {
                    let elapsed_secs = (time::OffsetDateTime::now_utc() - last)
                        .whole_seconds()
                        .max(0) as u64;
                    if elapsed_secs < delay.as_secs() {
                        tracing::debug!(
                            module = "captain",
                            pr = %pr,
                            delay_s = delay.as_secs(),
                            elapsed = elapsed_secs,
                            "rebase backoff, waiting"
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
    let project_name = item.project.as_str();
    let Some((_, project_config)) =
        settings::config::resolve_project_config(Some(project_name), config)
    else {
        tracing::warn!(
            module = "captain",
            pr = %pr,
            project = project_name,
            "skipping rebase — no project config found"
        );
        return;
    };

    let repo_path = global_infra::paths::expand_tilde(&project_config.path);
    let default_branch_raw = match crate::io::git::default_branch(&repo_path).await {
        Ok(db) => db,
        Err(e) => {
            tracing::warn!(
                module = "captain",
                pr = %pr,
                repo = %repo_path.display(),
                error = %e,
                "skipping rebase; default_branch lookup failed"
            );
            return;
        }
    };
    let default_branch = default_branch_raw
        .strip_prefix("origin/")
        .unwrap_or(&default_branch_raw)
        .to_string();
    let branch = item.branch.as_deref().unwrap_or("");
    let pr_num = crate::parse_pr_number(pr)
        .map(|n| n.to_string())
        .unwrap_or_else(|| pr.trim_start_matches('#').to_string());

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
    let attempt_str = (rebase_retries + 1).to_string();
    let max_retries_str = max_rebase_retries.to_string();
    let rebase_vars: rustc_hash::FxHashMap<&str, &str> = [
        ("branch", branch),
        ("default_branch", default_branch.as_str()),
        ("pr_num", pr_num.as_str()),
        ("attempt", attempt_str.as_str()),
        ("max_retries", max_retries_str.as_str()),
    ]
    .into_iter()
    .collect();
    let prompt = match settings::config::render_template(&rebase_tmpl, &rebase_vars) {
        Ok(p) => p,
        Err(e) => {
            tracing::error!(module = "captain", error = %e, "failed to render rebase_worker template");
            return;
        }
    };

    let wt = item.worktree.as_deref().unwrap_or("");
    let wt_path = global_infra::paths::expand_tilde(wt);

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
    let session_id = global_infra::uuid::Uuid::v4().to_string();

    // Pick credential so the rebase worker participates in load balancing.
    let credential = super::tick_spawn::pick_credential(pool, None).await;
    let cred_id = global_claude::credentials::credential_id(&credential);
    let mut env: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    if let Some((_id, token)) = &credential {
        env.insert("CLAUDE_CODE_OAUTH_TOKEN".into(), token.clone());
    }

    match crate::io::process_manager::spawn_worker_process(
        &prompt,
        &wt_path,
        &workflow.models.worker,
        &session_id,
        &env,
    )
    .await
    {
        Ok((pid, _)) => {
            // Register under both session_name (for reap_dead_rebase_workers
            // lifecycle) and session_id (for session reconciler + terminator).
            if let Err(e) = crate::io::pid_registry::register(&session_name, pid) {
                tracing::warn!(module = "captain", worker = %session_name, %e, "pid_registry register failed");
            }
            if let Err(e) = crate::io::pid_registry::register(&session_id, pid) {
                tracing::warn!(module = "captain", session_id = %session_id, %e, "pid_registry register (session_id) failed");
            }
            // Log "running" session entry so the UI shows it immediately.
            if let Err(e) = crate::io::headless_cc::log_running_session(
                pool,
                &session_id,
                &wt_path,
                "rebase",
                &session_name,
                Some(items[idx].id),
                false,
                cred_id,
            )
            .await
            {
                tracing::warn!(
                    module = "captain",
                    session_id = %session_id,
                    error = %e,
                    "failed to log rebase session"
                );
            }
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
                pid = %pid,
                "rebase worker spawned"
            );
            let _ = super::timeline_emit::emit_for_task(
                item,
                crate::TimelineEventType::RebaseTriggered,
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
