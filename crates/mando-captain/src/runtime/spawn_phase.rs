//! Spawn phase — execute captain actions (nudge, ship, captain-review).

use anyhow::Result;
use mando_config::settings::Config;
use mando_types::captain::{Action, ActionKind};
use mando_types::Task;

use mando_config::workflow::CaptainWorkflow;

use crate::biz::spawn_logic;
use crate::runtime::linear_integration;

/// Execute a single captain action against the live item list.
#[tracing::instrument(skip_all, fields(module = "captain", action_kind = ?action.action, worker = %action.worker))]
pub(crate) async fn execute_action(
    action: &Action,
    items: &mut [Task],
    config: &Config,
    workflow: &CaptainWorkflow,
    notifier: &super::notify::Notifier,
    alerts: &mut Vec<String>,
    pool: &sqlx::SqlitePool,
) -> Result<()> {
    match action.action {
        ActionKind::Skip => {}
        ActionKind::Nudge => {
            execute_nudge(action, items, config, workflow, notifier, alerts, pool).await?;
        }
        ActionKind::CaptainReview => {
            super::spawn_phase_review::handle_captain_review(
                action, items, config, workflow, notifier, pool,
            )
            .await?;
        }
        ActionKind::Ship => {
            execute_ship(action, items, config, workflow, notifier, alerts, pool).await?;
        }
    }
    Ok(())
}

/// Nudge: check budget, kill old process, resume with message.
async fn execute_nudge(
    action: &Action,
    items: &mut [Task],
    config: &Config,
    workflow: &CaptainWorkflow,
    notifier: &super::notify::Notifier,
    alerts: &mut Vec<String>,
    pool: &sqlx::SqlitePool,
) -> Result<()> {
    let item_data = items
        .iter()
        .find(|it| it.worker.as_deref() == Some(&action.worker))
        .cloned();

    let Some(item_clone) = item_data else {
        return Ok(());
    };

    // Check intervention budget.
    let intervention_count = item_clone.intervention_count as u32;
    let budget =
        spawn_logic::check_intervention(intervention_count, 1, workflow.agent.max_interventions);

    match budget {
        spawn_logic::InterventionResult::Exhausted { new_count } => {
            // Budget exhausted → trigger CaptainReview instead of escalating.
            tracing::warn!(
                module = "captain",
                worker = %action.worker,
                intervention_count = new_count,
                max = workflow.agent.max_interventions,
                "intervention budget exhausted — triggering captain review"
            );
            super::spawn_phase_review::trigger_captain_review(
                action,
                items,
                config,
                workflow,
                notifier,
                "budget_exhausted",
                pool,
            )
            .await;
            return Ok(());
        }
        spawn_logic::InterventionResult::Proceed { new_count } => {
            execute_nudge_resume(
                action,
                items,
                &item_clone,
                new_count,
                config,
                workflow,
                notifier,
                alerts,
                pool,
            )
            .await?;
        }
    }

    Ok(())
}

/// Inner nudge logic: kill old process, check broken session, resume.
#[allow(clippy::too_many_arguments)]
async fn execute_nudge_resume(
    action: &Action,
    items: &mut [Task],
    item_clone: &Task,
    new_count: u32,
    config: &Config,
    workflow: &CaptainWorkflow,
    notifier: &super::notify::Notifier,
    alerts: &mut Vec<String>,
    pool: &sqlx::SqlitePool,
) -> Result<()> {
    let cc_sid = item_clone.session_ids.worker.as_deref().unwrap_or("");
    let wt = item_clone.worktree.as_deref().unwrap_or("");

    if cc_sid.is_empty() || wt.is_empty() {
        return Ok(());
    }

    let msg_owned;
    let msg = match action.message.as_deref() {
        Some(m) if !m.is_empty() => m,
        _ => {
            msg_owned = mando_config::render_nudge(
                "nudge_default",
                &workflow.nudges,
                &std::collections::HashMap::new(),
            )
            .map_err(|e| anyhow::anyhow!(e))?;
            &msg_owned
        }
    };
    let wt_path = mando_config::expand_tilde(wt);
    let env = std::collections::HashMap::new();

    // Kill the existing worker process before resuming.
    let old_pid = crate::io::health_store::get_pid_for_worker(&action.worker);
    if old_pid > 0 {
        match mando_cc::kill_process(old_pid).await {
            Ok(()) => {
                tracing::debug!(module = "captain", worker = %action.worker, pid = old_pid, "killed old process before nudge")
            }
            Err(e) => {
                tracing::warn!(module = "captain", worker = %action.worker, pid = old_pid, error = %e, "failed to kill old process before nudge")
            }
        }
    }

    // Broken session: stream has content but no init event — CC never started.
    // Trigger CaptainReview directly; the classifier can miss this case when
    // an error result event exists without an init event.
    let stream_path = mando_config::stream_path_for_session(cc_sid);
    if mando_cc::stream_has_broken_session(&stream_path) {
        tracing::warn!(
            module = "captain",
            worker = %action.worker,
            cc_sid,
            "no init event in stream — triggering captain review"
        );
        super::spawn_phase_review::trigger_captain_review(
            action,
            items,
            config,
            workflow,
            notifier,
            "broken_session",
            pool,
        )
        .await;
        alerts.push(format!(
            "Broken session for {} — captain review triggered",
            action.worker,
        ));
        return Ok(());
    }

    // Record stream file size before resume for zero-byte detection.
    let stream_size_before = mando_cc::get_stream_file_size(&stream_path);

    match crate::io::process_manager::resume_worker_process(
        &action.worker,
        msg,
        &wt_path,
        &workflow.models.worker,
        cc_sid,
        &env,
        workflow.models.fallback.as_deref(),
    )
    .await
    {
        Ok((pid, _)) => {
            persist_nudge_health(action, pid, stream_size_before, new_count);
            crate::io::headless_cc::log_running_session(
                pool,
                cc_sid,
                &wt_path,
                "worker",
                &action.worker,
                &item_clone.best_id(),
                true,
            )
            .await;

            // Update intervention_count on the live item.
            if let Some(it) = items
                .iter_mut()
                .find(|it| it.worker.as_deref() == Some(&action.worker))
            {
                it.intervention_count = new_count as i64;
            }

            // Emit timeline event with the actual nudge message.
            super::timeline_emit::emit_for_task(
                item_clone,
                mando_types::timeline::TimelineEventType::WorkerNudged,
                &format!(
                    "Nudged {} ({}/{})",
                    action.worker, new_count, workflow.agent.max_interventions
                ),
                serde_json::json!({
                    "worker": action.worker,
                    "session_id": cc_sid,
                    "content": msg,
                    "nudge_count": new_count,
                }),
                pool,
            )
            .await;

            tracing::info!(
                module = "captain",
                worker = %action.worker,
                pid = pid,
                intervention_count = new_count,
                max = workflow.agent.max_interventions,
                "nudged worker"
            );

            // Linear writeback.
            if let Err(e) = linear_integration::writeback_status(item_clone, config).await {
                tracing::warn!(module = "captain", %e, "Linear status writeback failed");
            }
            if let Err(e) = linear_integration::upsert_workpad(
                item_clone,
                config,
                &format!(
                    "Nudged (#{}/{})",
                    new_count, workflow.agent.max_interventions
                ),
                pool,
            )
            .await
            {
                tracing::warn!(module = "captain", %e, "Linear workpad upsert failed");
            }
        }
        Err(e) => {
            // Resume failed — persist count so budget tracks failures too.
            // Don't retry — next tick will detect broken session → CaptainReview.
            crate::io::health_store::persist_nudge_count(&action.worker, new_count);
            super::timeline_emit::emit_for_task(
                item_clone,
                mando_types::timeline::TimelineEventType::WorkerNudged,
                &format!(
                    "Nudge failed for {} ({}/{}): {}",
                    action.worker, new_count, workflow.agent.max_interventions, e
                ),
                serde_json::json!({
                    "worker": action.worker,
                    "session_id": cc_sid,
                    "nudge_count": new_count,
                    "error": e.to_string(),
                }),
                pool,
            )
            .await;
            tracing::warn!(
                module = "captain",
                worker = %action.worker,
                intervention_count = new_count,
                error = %e,
                "nudge resume failed"
            );
        }
    }

    Ok(())
}

/// Persist health state after a successful nudge resume.
fn persist_nudge_health(action: &Action, pid: u32, stream_size_before: u64, new_count: u32) {
    let health_path = mando_config::worker_health_path();
    let mut hstate = crate::io::health_store::load_health_state(&health_path);
    crate::io::health_store::set_health_field(
        &mut hstate,
        &action.worker,
        "pid",
        serde_json::json!(pid),
    );
    crate::io::health_store::set_health_field(
        &mut hstate,
        &action.worker,
        "stream_size_at_spawn",
        serde_json::json!(stream_size_before),
    );
    crate::io::health_store::set_health_field(
        &mut hstate,
        &action.worker,
        "nudge_count",
        serde_json::json!(new_count),
    );
    if let Err(e) = crate::io::health_store::save_health_state(&health_path, &hstate) {
        tracing::error!(module = "captain", worker = %action.worker, error = %e, "failed to persist health state");
    }
}

/// Ship: kill worker, teardown, set status to AwaitingReview/CompletedNoPr.
async fn execute_ship(
    action: &Action,
    items: &mut [Task],
    config: &Config,
    _workflow: &CaptainWorkflow,
    notifier: &super::notify::Notifier,
    _alerts: &mut Vec<String>,
    pool: &sqlx::SqlitePool,
) -> Result<()> {
    let Some(it) = items
        .iter_mut()
        .find(|it| it.worker.as_deref() == Some(&action.worker))
    else {
        return Ok(());
    };

    // Per-item lock prevents interleaving with concurrent dashboard ops.
    let item_id = it.id.to_string();
    let _lock = match crate::io::item_lock::acquire_item_lock(&item_id, "tick-ship") {
        Ok(lock) => lock,
        Err(e) => {
            tracing::warn!(
                module = "captain",
                worker = %action.worker,
                item_id = %item_id,
                error = %e,
                "item lock blocked, skipping ship transition"
            );
            return Ok(());
        }
    };

    // Kill worker process.
    let pid = crate::io::health_store::get_pid_for_worker(&action.worker);
    if pid > 0 {
        if let Err(e) = mando_cc::kill_process(pid).await {
            tracing::warn!(module = "captain", worker = %action.worker, pid = pid, error = %e, "failed to kill worker for ship");
        }
    }

    // Run teardown hook.
    if let Some(wt) = &it.worktree {
        let wt_path = mando_config::expand_tilde(wt);
        if let Some((_, project_config)) =
            mando_config::resolve_project_config(it.project.as_deref(), config)
        {
            if let Err(e) = crate::io::hooks::teardown(&project_config.hooks, &wt_path).await {
                tracing::warn!(module = "captain", worker = %action.worker, error = %e, "teardown hook failed");
            }
        }
    }

    // Update status.
    let is_no_pr = it.no_pr;
    it.status = spawn_logic::ship_status(is_no_pr);

    // Emit timeline event.
    super::timeline_emit::emit_for_task(
        it,
        mando_types::timeline::TimelineEventType::AwaitingReview,
        &format!(
            "Worker done ({})",
            it.pr
                .as_deref()
                .map(mando_shared::helpers::pr_short_label)
                .as_deref()
                .unwrap_or("no PR")
        ),
        serde_json::json!({"worker": action.worker, "session_id": it.session_ids.worker, "pr": it.pr}),
        pool,
    )
    .await;

    crate::io::headless_cc::log_item_session(
        pool,
        it,
        &action.worker,
        mando_types::SessionStatus::Stopped,
    )
    .await;

    // Notify via Telegram.
    let pr_ref = it.pr.as_deref().map(mando_shared::helpers::pr_short_label);
    let pr_ref = pr_ref.as_deref().unwrap_or("no PR");
    let msg = format!(
        "\u{2705} Awaiting review ({}): <b>{}</b>",
        pr_ref,
        mando_shared::telegram_format::escape_html(&it.title),
    );
    notifier.high(&msg).await;
    tracing::info!(module = "captain", worker = %action.worker, "transitioned to awaiting-review");

    // Linear writeback.
    if let Err(e) = linear_integration::writeback_status(it, config).await {
        tracing::warn!(module = "captain", %e, "Linear status writeback failed");
    }
    if let Err(e) = linear_integration::upsert_workpad(
        it,
        config,
        &format!("Worker done, awaiting review ({})", pr_ref),
        pool,
    )
    .await
    {
        tracing::warn!(module = "captain", %e, "Linear workpad upsert failed");
    }

    Ok(())
}
