//! Shared captain action execution for manual and automatic flows.

use anyhow::{bail, Result};
use mando_config::settings::Config;
use mando_config::workflow::CaptainWorkflow;
use mando_types::task::{ItemStatus, ReviewTrigger, Task};

use crate::biz::spawn_logic;
use crate::runtime::task_notes::append_tagged_note;

use super::{
    captain_review, linear_integration, notify::Notifier, spawner_lifecycle, timeline_emit,
};

pub enum ReopenOutcome {
    Reopened,
    QueuedFallback,
    CaptainReviewing,
}

pub async fn nudge_item(
    item: &mut Task,
    message: Option<&str>,
    config: &Config,
    workflow: &CaptainWorkflow,
    notifier: &Notifier,
    alerts: &mut Vec<String>,
    pool: &sqlx::SqlitePool,
) -> Result<()> {
    let item_id = item.id.to_string();
    let _lock = crate::io::item_lock::acquire_item_lock(&item_id, "nudge")?;
    let worker = item
        .worker
        .clone()
        .ok_or_else(|| anyhow::anyhow!("item has no worker"))?;
    let cc_sid = item
        .session_ids
        .worker
        .clone()
        .ok_or_else(|| anyhow::anyhow!("item has no worker session"))?;
    let wt = item
        .worktree
        .clone()
        .ok_or_else(|| anyhow::anyhow!("item has no worktree"))?;

    let budget = spawn_logic::check_intervention(
        item.intervention_count as u32,
        1,
        workflow.agent.max_interventions,
    );
    let new_count = match budget {
        spawn_logic::InterventionResult::Proceed { new_count } => new_count,
        spawn_logic::InterventionResult::Exhausted { new_count } => {
            item.intervention_count = new_count as i64;
            item.last_activity_at = Some(mando_types::now_rfc3339());
            trigger_review(
                item,
                ReviewTrigger::BudgetExhausted,
                config,
                workflow,
                notifier,
                pool,
            )
            .await?;
            return Ok(());
        }
    };

    let msg_owned;
    let msg = match message {
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

    let old_pid = crate::io::pid_lookup::resolve_pid(&cc_sid, &worker).unwrap_or(0);
    if old_pid > 0 {
        if let Err(e) = mando_cc::kill_process(old_pid).await {
            tracing::warn!(module = "captain", worker = %worker, pid = old_pid, error = %e, "failed to kill old process before nudge");
        }
    }

    let stream_path = mando_config::stream_path_for_session(&cc_sid);
    if mando_cc::stream_has_broken_session(&stream_path) {
        item.intervention_count = new_count as i64;
        trigger_review(
            item,
            ReviewTrigger::BrokenSession,
            config,
            workflow,
            notifier,
            pool,
        )
        .await?;
        alerts.push(format!(
            "Broken session for {} — captain review triggered",
            worker
        ));
        return Ok(());
    }

    let stream_size_before = mando_cc::get_stream_file_size(&stream_path);
    let wt_path = mando_config::expand_tilde(&wt);
    let env = std::collections::HashMap::new();

    match crate::io::process_manager::resume_worker_process(
        &worker,
        msg,
        &wt_path,
        &workflow.models.worker,
        &cc_sid,
        &env,
        workflow.models.fallback.as_deref(),
    )
    .await
    {
        Ok((pid, _)) => {
            persist_nudge_health(&cc_sid, &worker, pid, stream_size_before, new_count);
            crate::io::headless_cc::log_running_session(
                pool,
                &cc_sid,
                &wt_path,
                "worker",
                &worker,
                &item.best_id(),
                true,
            )
            .await;

            item.intervention_count = new_count as i64;
            timeline_emit::emit_for_task(
                item,
                mando_types::timeline::TimelineEventType::WorkerNudged,
                &format!(
                    "Nudged {} ({}/{})",
                    worker, new_count, workflow.agent.max_interventions
                ),
                serde_json::json!({
                    "worker": worker,
                    "session_id": cc_sid,
                    "content": msg,
                    "nudge_count": new_count,
                }),
                pool,
            )
            .await;

            if let Err(e) = linear_integration::writeback_status(item, config).await {
                tracing::warn!(module = "captain", %e, "Linear status writeback failed");
            }
            if let Err(e) = linear_integration::upsert_workpad(
                item,
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
            crate::io::health_store::persist_nudge_count(&worker, new_count);
            item.intervention_count = new_count as i64;
            timeline_emit::emit_for_task(
                item,
                mando_types::timeline::TimelineEventType::WorkerNudged,
                &format!(
                    "Nudge failed for {} ({}/{}): {}",
                    worker, new_count, workflow.agent.max_interventions, e
                ),
                serde_json::json!({
                    "worker": worker,
                    "session_id": cc_sid,
                    "nudge_count": new_count,
                    "error": e.to_string(),
                }),
                pool,
            )
            .await;
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub async fn reopen_item(
    item: &mut Task,
    reopen_source: &str,
    feedback: &str,
    config: &Config,
    workflow: &CaptainWorkflow,
    notifier: &Notifier,
    pool: &sqlx::SqlitePool,
    allow_queue_fallback: bool,
) -> Result<ReopenOutcome> {
    // Reject reopens on items being actively managed by captain.
    if reopen_source == "human"
        && (item.status == ItemStatus::CaptainReviewing
            || item.status == ItemStatus::CaptainMerging)
    {
        anyhow::bail!(
            "cannot reopen item {}: captain {} is in progress",
            item.id,
            if item.status == ItemStatus::CaptainReviewing {
                "review"
            } else {
                "merge"
            }
        );
    }

    let item_id = item.id.to_string();
    let _lock = crate::io::item_lock::acquire_item_lock(&item_id, "reopen")?;

    if let Some(new_context) =
        append_tagged_note(item.context.as_deref(), "Reopen feedback", feedback)
    {
        item.context = Some(new_context);
    }

    let budget = spawn_logic::check_intervention(
        item.intervention_count as u32,
        1,
        workflow.agent.max_interventions,
    );
    let new_count = match budget {
        spawn_logic::InterventionResult::Proceed { new_count } => new_count,
        spawn_logic::InterventionResult::Exhausted { new_count } => {
            item.intervention_count = new_count as i64;
            item.last_activity_at = Some(mando_types::now_rfc3339());
            trigger_review(
                item,
                ReviewTrigger::BudgetExhausted,
                config,
                workflow,
                notifier,
                pool,
            )
            .await?;
            return Ok(ReopenOutcome::CaptainReviewing);
        }
    };

    item.reopen_source = Some(reopen_source.to_string());
    let can_resume =
        item.worker.is_some() && item.session_ids.worker.is_some() && item.worktree.is_some();
    if !can_resume {
        if allow_queue_fallback {
            item.intervention_count = new_count as i64;
            item.reopen_seq += 1;
            item.status = ItemStatus::Queued;
            item.last_activity_at = Some(mando_types::now_rfc3339());
            return Ok(ReopenOutcome::QueuedFallback);
        }
        bail!("item missing worker/session/worktree — cannot reopen");
    }

    match spawner_lifecycle::reopen_worker(item, config, feedback, workflow, pool).await {
        Ok(result) => {
            item.intervention_count = new_count as i64;
            item.reopen_seq += 1;
            item.status = ItemStatus::InProgress;
            item.worker = Some(result.session_name);
            item.session_ids.worker = Some(result.session_id);
            item.branch = Some(result.branch);
            item.worktree = Some(result.worktree);
            item.last_activity_at = Some(mando_types::now_rfc3339());

            if let Err(e) = linear_integration::writeback_status(item, config).await {
                tracing::warn!(module = "captain", %e, "Linear status writeback failed");
            }

            Ok(ReopenOutcome::Reopened)
        }
        Err(e) => {
            tracing::warn!(
                module = "captain",
                item_id = item.id,
                error = %e,
                "reopen_worker failed — falling back to queue"
            );
            if allow_queue_fallback {
                item.intervention_count = new_count as i64;
                item.reopen_seq += 1;
                item.status = ItemStatus::Queued;
                item.last_activity_at = Some(mando_types::now_rfc3339());
                Ok(ReopenOutcome::QueuedFallback)
            } else {
                Err(e)
            }
        }
    }
}

pub(crate) fn reset_review_retry(item: &mut Task, trigger: ReviewTrigger) {
    item.status = ItemStatus::CaptainReviewing;
    item.captain_review_trigger = Some(trigger);
    item.session_ids.review = None;
    item.review_fail_count = 0;
    item.last_activity_at = Some(mando_types::now_rfc3339());
}

async fn trigger_review(
    item: &mut Task,
    trigger: ReviewTrigger,
    config: &Config,
    workflow: &CaptainWorkflow,
    notifier: &Notifier,
    pool: &sqlx::SqlitePool,
) -> Result<()> {
    reset_review_retry(item, trigger);
    captain_review::spawn_review(item, trigger.as_str(), config, workflow, notifier, pool).await?;
    if let Err(e) = linear_integration::writeback_status(item, config).await {
        tracing::warn!(module = "captain", %e, "Linear status writeback failed");
    }
    if let Err(e) = linear_integration::upsert_workpad(
        item,
        config,
        &format!("Captain reviewing ({})", trigger.as_str()),
        pool,
    )
    .await
    {
        tracing::warn!(module = "captain", %e, "Linear workpad upsert failed");
    }
    Ok(())
}

fn persist_nudge_health(
    session_id: &str,
    worker: &str,
    pid: u32,
    stream_size_before: u64,
    new_count: u32,
) {
    crate::io::pid_registry::register(session_id, pid);
    let health_path = mando_config::worker_health_path();
    let mut hstate = crate::io::health_store::load_health_state(&health_path);
    crate::io::health_store::set_health_field(&mut hstate, worker, "pid", serde_json::json!(pid));
    crate::io::health_store::set_health_field(
        &mut hstate,
        worker,
        "stream_size_at_spawn",
        serde_json::json!(stream_size_before),
    );
    crate::io::health_store::set_health_field(
        &mut hstate,
        worker,
        "nudge_count",
        serde_json::json!(new_count),
    );
    if let Err(e) = crate::io::health_store::save_health_state(&health_path, &hstate) {
        tracing::error!(module = "captain", worker = %worker, error = %e, "failed to persist health state");
    }
}
