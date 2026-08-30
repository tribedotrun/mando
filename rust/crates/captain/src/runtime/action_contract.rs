//! Shared captain action execution for manual and automatic flows.

use crate::{ItemStatus, ReviewTrigger, Task};
use anyhow::{Context, Result};
use rustc_hash::FxHashMap;
use settings::CaptainWorkflow;
use settings::Config;

use crate::service::{lifecycle, spawn_logic};

use super::{captain_review, notify::Notifier, spawner_lifecycle, timeline_emit};

mod nudge_reason;
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

    // ── Circuit breaker: repeated nudge reason KIND → captain review ──
    // Keyed on the stable kind, never the formatted text: reasons like
    // `gates incomplete: <failures>` and `PR has <n> unresolved review
    // thread(s)` embed live data, so exact-text comparison could never
    // observe a repeat and the loop fell through to `max_interventions`.
    let mut nudge_reason_key: Option<&str> = None;
    if let Some(reason_str) = reason {
        let health_path = crate::config::worker_health_path();
        let hstate = crate::io::health_store::load_health_state(&health_path)
            .with_context(|| format!("load health state from {}", health_path.display()))?;
        let last_key =
            crate::io::health_store::get_health_str(&hstate, &worker, "last_nudge_reason");
        let consecutive =
            crate::io::health_store::get_health_u32(&hstate, &worker, "nudge_reason_consecutive");
        let step = nudge_reason::advance(last_key.as_deref(), reason_str, consecutive);
        nudge_reason_key = Some(step.key);

        if step.consecutive >= workflow.agent.max_repeated_nudges {
            tracing::info!(
                module = "captain",
                worker = %worker,
                reason = %reason_str,
                reason_kind = %step.key,
                consecutive = step.consecutive,
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
                msg_owned = settings::render_nudge("nudge_default", &workflow.nudges, &empty_vars)
                    .map_err(|e| anyhow::anyhow!(e))?;
                &msg_owned
            }
        },
    };

    let old_pid =
        crate::io::pid_lookup::resolve_pid(&cc_sid, &worker).unwrap_or(crate::Pid::new(0));
    let wt_path = global_infra::paths::expand_tilde(&wt);

    match super::agent_runtime::nudge_worker(
        pool,
        item,
        &worker,
        &wt_path,
        msg,
        &cc_sid,
        &workflow.models.worker,
        workflow,
        old_pid,
    )
    .await
    {
        Ok(super::agent_runtime::AgentNudgeOutcome::Delivered(delivery)) => {
            // Persist the stable kind, not the formatted reason, so the
            // write side matches what the breaker above compares.
            persist_nudge_health(
                &cc_sid,
                &worker,
                delivery.pid,
                delivery.stream_size_before,
                new_count,
                nudge_reason_key,
            )?;

            // Clear AI feedback only after the nudge was successfully delivered.
            if ai_feedback.is_some() {
                crate::io::health_store::persist_health_field(
                    &worker,
                    "pending_ai_feedback",
                    serde_json::Value::Null,
                    "failed to clear pending_ai_feedback; next nudge may re-deliver stale feedback",
                );
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
        Ok(super::agent_runtime::AgentNudgeOutcome::BrokenSession { alert }) => {
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
            alerts.push(alert);
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
