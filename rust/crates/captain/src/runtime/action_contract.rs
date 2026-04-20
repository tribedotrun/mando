//! Shared captain action execution for manual and automatic flows.

use crate::{ItemStatus, ReviewTrigger, Task};
use anyhow::{Context, Result};
use rustc_hash::FxHashMap;
use settings::config::settings::Config;
use settings::config::workflow::CaptainWorkflow;

use crate::service::{lifecycle, spawn_logic};

use super::{captain_review, notify::Notifier, spawner_lifecycle, timeline_emit};

mod reopen;

pub use reopen::{reopen_item, ReopenOutcome};

#[allow(clippy::too_many_arguments)]
#[tracing::instrument(skip_all)]
pub async fn nudge_item(
    item: &mut Task,
    message: Option<&str>,
    reason: Option<&str>,
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
            item.last_activity_at = Some(global_types::now_rfc3339());
            reopen::trigger_review(
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

    // ── Circuit breaker: repeated identical nudge reason → captain review ──
    if let Some(reason_str) = reason {
        let health_path = crate::config::worker_health_path();
        let hstate = crate::io::health_store::load_health_state(&health_path)
            .with_context(|| format!("load health state from {}", health_path.display()))?;
        let last_reason =
            crate::io::health_store::get_health_str(&hstate, &worker, "last_nudge_reason");
        let consecutive =
            crate::io::health_store::get_health_u32(&hstate, &worker, "nudge_reason_consecutive");
        let same = last_reason.as_deref() == Some(reason_str);
        let new_consecutive = if same { consecutive + 1 } else { 1 };

        if new_consecutive >= workflow.agent.max_repeated_nudges {
            tracing::info!(
                module = "captain",
                worker = %worker,
                reason = %reason_str,
                consecutive = new_consecutive,
                "repeated-nudge circuit breaker: routing to captain review"
            );
            item.intervention_count = new_count as i64;
            reopen::trigger_review(
                item,
                ReviewTrigger::RepeatedNudge,
                config,
                workflow,
                notifier,
                pool,
            )
            .await?;
            // Reset counter after review is started so the worker gets a
            // fresh window. Placed after trigger_review so a failure leaves
            // the counter at the threshold for retry on the next tick.
            crate::io::health_store::persist_health_field(
                &worker,
                "nudge_reason_consecutive",
                serde_json::json!(0),
                "failed to reset circuit breaker counter",
            );
            return Ok(());
        }
    }

    // Message priority: pending AI feedback > classifier template > nudge_default.
    // AI feedback takes precedence because the captain review has full context and
    // its instructions are more specific than any template the classifier produces.
    // Read but don't clear yet — clear only after nudge is successfully delivered,
    // so the feedback survives if this function exits early (broken session, etc.).
    let ai_feedback = {
        let health_path = crate::config::worker_health_path();
        let hstate = crate::io::health_store::load_health_state(&health_path)
            .with_context(|| format!("load health state from {}", health_path.display()))?;
        crate::io::health_store::get_health_str(&hstate, &worker, "pending_ai_feedback")
    };

    let msg_owned;
    let msg = match ai_feedback.as_deref() {
        Some(fb) if !fb.is_empty() => {
            msg_owned = fb.to_string();
            &msg_owned
        }
        _ => match message {
            Some(m) if !m.is_empty() => m,
            _ => {
                let empty_vars: FxHashMap<&str, &str> = FxHashMap::default();
                msg_owned =
                    settings::config::render_nudge("nudge_default", &workflow.nudges, &empty_vars)
                        .map_err(|e| anyhow::anyhow!(e))?;
                &msg_owned
            }
        },
    };

    let old_pid =
        crate::io::pid_lookup::resolve_pid(&cc_sid, &worker).unwrap_or(crate::Pid::new(0));
    if old_pid.as_u32() > 0 {
        if let Err(e) = global_claude::kill_process(old_pid).await {
            tracing::warn!(module = "captain", worker = %worker, pid = %old_pid, error = %e, "failed to kill old process before nudge");
        }
    }

    let stream_path = global_infra::paths::stream_path_for_session(&cc_sid);
    if global_claude::stream_has_broken_session(&stream_path) {
        item.intervention_count = new_count as i64;
        reopen::trigger_review(
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

    let stream_size_before = global_claude::get_stream_file_size(&stream_path);
    let wt_path = global_infra::paths::expand_tilde(&wt);
    let (env, cred_id) = super::spawner::credential_env_for_session(pool, &cc_sid).await;

    match crate::io::process_manager::resume_worker_process(
        msg,
        &wt_path,
        &workflow.models.worker,
        &cc_sid,
        &env,
    )
    .await
    {
        Ok((pid, _)) => {
            persist_nudge_health(&cc_sid, &worker, pid, stream_size_before, new_count, reason)?;

            // Clear AI feedback only after the nudge was successfully delivered.
            if ai_feedback.is_some() {
                crate::io::health_store::persist_health_field(
                    &worker,
                    "pending_ai_feedback",
                    serde_json::Value::Null,
                    "failed to clear pending_ai_feedback; next nudge may re-deliver stale feedback",
                );
            }

            if let Err(e) = crate::io::headless_cc::log_running_session(
                pool,
                &cc_sid,
                &wt_path,
                "worker",
                &worker,
                Some(item.id),
                true,
                cred_id,
            )
            .await
            {
                tracing::warn!(module = "captain", worker = %worker, %e, "failed to log running session after nudge");
            }

            item.intervention_count = new_count as i64;
            global_infra::best_effort!(
                timeline_emit::emit_for_task(
                    item,
                    &format!(
                        "Nudged {} ({}/{})",
                        worker, new_count, workflow.agent.max_interventions
                    ),
                    crate::TimelineEventPayload::WorkerNudged {
                        worker: worker.clone(),
                        session_id: cc_sid.clone(),
                        content: msg.to_string(),
                        reason: reason.unwrap_or("").to_string(),
                        nudge_count: new_count as i64,
                    },
                    pool,
                )
                .await,
                "action_contract: timeline_emit::emit_for_task( item, &format!( 'Nudged {} ({}"
            );
            Ok(())
        }
        Err(e) => {
            // Nudge delivery failed; do NOT increment intervention_count.
            // The budget must only decrement on successful interventions so
            // transient resume failures don't burn the worker's budget.
            global_infra::best_effort!(
                timeline_emit::emit_for_task(
                    item,
                    &format!(
                        "Nudge delivery failed for {} ({}/{}): {}",
                        worker, new_count, workflow.agent.max_interventions, e
                    ),
                    crate::TimelineEventPayload::WorkerNudgeFailed {
                        worker: worker.clone(),
                        session_id: cc_sid.clone(),
                        reason: reason.unwrap_or("").to_string(),
                        nudge_count_attempted: new_count as i64,
                        error: e.to_string(),
                    },
                    pool,
                )
                .await,
                "action_contract: timeline_emit::emit_for_task( item, &format!( 'Nudge deliver"
            );
            Err(anyhow::anyhow!("nudge delivery failed for {worker}: {e}"))
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn reset_review_retry(item: &mut Task, trigger: ReviewTrigger) {
    if let Err(e) = lifecycle::apply_transition(item, ItemStatus::CaptainReviewing) {
        tracing::error!(
            module = "captain",
            item_id = item.id,
            from = %item.status.as_str(),
            trigger = %trigger.as_str(),
            error = %e,
            "illegal reset_review_retry transition"
        );
        return;
    }
    item.captain_review_trigger = Some(trigger);
    item.session_ids.review = None;
    item.review_fail_count = 0;
    item.last_activity_at = Some(global_types::now_rfc3339());
}
use super::nudge_health::persist_nudge_health;
pub(crate) use super::review_snapshot::ReviewFieldsSnapshot;
