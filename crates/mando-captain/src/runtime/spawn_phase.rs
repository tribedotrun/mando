//! Spawn phase — execute captain actions (nudge, ship, captain-review).

use anyhow::Result;
use mando_config::settings::Config;
use mando_types::captain::{Action, ActionKind};
use mando_types::Task;

use crate::biz::spawn_logic;
use mando_config::workflow::CaptainWorkflow;

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
    let Some(item) = items
        .iter_mut()
        .find(|it| it.worker.as_deref() == Some(&action.worker))
    else {
        return Ok(());
    };
    super::action_contract::nudge_item(
        item,
        action.message.as_deref(),
        action.reason.as_deref(),
        config,
        workflow,
        notifier,
        alerts,
        pool,
    )
    .await
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
    let cc_sid = it.session_ids.worker.as_deref().unwrap_or("");
    let pid = crate::io::pid_lookup::resolve_pid(cc_sid, &action.worker)
        .unwrap_or(mando_types::Pid::new(0));
    if pid.as_u32() > 0 {
        if let Err(e) = mando_cc::kill_process(pid).await {
            tracing::warn!(module = "captain", worker = %action.worker, pid = %pid, error = %e, "failed to kill worker for ship");
        }
    }

    // Run teardown hook.
    if let Some(wt) = &it.worktree {
        let wt_path = mando_config::expand_tilde(wt);
        if let Some((_, project_config)) =
            mando_config::resolve_project_config(Some(&it.project), config)
        {
            if let Err(e) = crate::io::hooks::teardown(&project_config.hooks, &wt_path).await {
                tracing::warn!(module = "captain", worker = %action.worker, error = %e, "teardown hook failed");
            }
        }
    }

    // Update status.
    let prev_status = it.status;
    let is_no_pr = it.no_pr;
    it.status = spawn_logic::ship_status(is_no_pr);

    let pr_label_str = it.pr_number.map(mando_shared::helpers::pr_short_label);
    let event = mando_types::timeline::TimelineEvent {
        event_type: mando_types::timeline::TimelineEventType::AwaitingReview,
        timestamp: mando_types::now_rfc3339(),
        actor: "captain".to_string(),
        summary: format!(
            "Worker done ({})",
            pr_label_str.as_deref().unwrap_or("no PR")
        ),
        data: serde_json::json!({"worker": action.worker, "session_id": it.session_ids.worker, "pr_number": it.pr_number}),
    };
    match mando_db::queries::tasks::persist_status_transition(
        pool,
        it,
        prev_status.as_str(),
        &event,
    )
    .await
    {
        Ok(true) => {
            if let Err(e) = crate::io::headless_cc::log_item_session(
                pool,
                it,
                &action.worker,
                mando_types::SessionStatus::Stopped,
            )
            .await
            {
                tracing::warn!(module = "captain", worker = %action.worker, %e, "failed to log stopped worker session");
            }
            let pr_ref = pr_label_str.as_deref().unwrap_or("no PR");
            let msg = format!(
                "\u{2705} Awaiting review ({}): <b>{}</b>",
                pr_ref,
                mando_shared::telegram_format::escape_html(&it.title),
            );
            notifier.high(&msg).await;
            tracing::info!(module = "captain", worker = %action.worker, "transitioned to awaiting-review");
        }
        Ok(false) => {
            tracing::info!(module = "captain", worker = %action.worker, "ship transition already applied");
        }
        Err(e) => {
            it.status = prev_status;
            tracing::error!(module = "captain", worker = %action.worker, error = %e, "persist failed for ship");
        }
    }

    Ok(())
}
