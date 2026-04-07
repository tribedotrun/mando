//! Verdict application logic extracted from captain_review.

use anyhow::Result;
use tracing::warn;

use mando_config::settings::Config;
use mando_config::workflow::CaptainWorkflow;
use mando_types::task::{ItemStatus, Task};
use mando_types::timeline::TimelineEventType;
use mando_types::SessionStatus;

use sqlx::SqlitePool;

use super::captain_review_helpers::{escaped_title, inline_resume_worker};
use super::notify::Notifier;
use super::timeline_emit;
use crate::biz::spawn_logic;

/// Apply a captain review verdict to an item.
pub async fn apply_verdict(
    item: &mut Task,
    verdict: &super::captain_review::CaptainVerdict,
    _config: &Config,
    workflow: &CaptainWorkflow,
    notifier: &Notifier,
    pool: &SqlitePool,
) -> Result<()> {
    // Capture worker info before any arm clears it (respawn sets these to None).
    let worker_session_id = item.session_ids.worker.clone();
    let worker_name = item.worker.clone().unwrap_or_default();

    let data = serde_json::json!({ "action": verdict.action, "feedback": verdict.feedback });
    let title = escaped_title(item);
    let prev_status = item.status;

    // Outcome tracking. log_stopped_after tells the post-match block to mark
    // the worker session stopped (covers ship/escalate/retry_clarifier/other).
    // clear_review_fields is false only when nudge resume fails, so the next
    // tick can retry the same verdict.
    let mut log_stopped_after = false;
    let mut clear_review_fields = true;
    // Track whether the atomic persist succeeded so we can skip post-match
    // side effects on idempotent skip or DB error.
    let mut transition_applied = true;

    match verdict.action.as_str() {
        "ship" => {
            let is_no_pr = item.no_pr;
            item.status = spawn_logic::ship_status(is_no_pr);
            let (event_type, msg_suffix) = if is_no_pr {
                (TimelineEventType::CompletedNoPr, "completed (no PR)")
            } else {
                (TimelineEventType::AwaitingReview, "ready for review")
            };
            let event = mando_types::timeline::TimelineEvent {
                event_type,
                timestamp: mando_types::now_rfc3339(),
                actor: "captain".to_string(),
                summary: format!("Captain approved; {msg_suffix}"),
                data,
            };
            match mando_db::queries::tasks::persist_status_transition(
                pool,
                item,
                prev_status.as_str(),
                &event,
            )
            .await
            {
                Ok(true) => {
                    notifier
                        .high(&format!(
                            "\u{2705} Captain approved <b>{title}</b>; {msg_suffix}"
                        ))
                        .await;
                }
                Ok(false) => {
                    transition_applied = false;
                }
                Err(e) => {
                    item.status = prev_status;
                    transition_applied = false;
                    tracing::error!(module = "captain", item_id = item.id, error = %e, "persist failed for ship verdict");
                }
            }
            log_stopped_after = transition_applied;
        }
        "nudge" => {
            item.status = ItemStatus::InProgress;
            item.intervention_count += 1;
            item.worker_started_at = Some(mando_types::now_rfc3339());
            let event = mando_types::timeline::TimelineEvent {
                event_type: TimelineEventType::CaptainReviewVerdict,
                timestamp: mando_types::now_rfc3339(),
                actor: "captain".to_string(),
                summary: format!("Captain nudge: {}", verdict.feedback),
                data,
            };
            match mando_db::queries::tasks::persist_status_transition(
                pool,
                item,
                prev_status.as_str(),
                &event,
            )
            .await
            {
                Ok(true) => {
                    notifier
                        .normal(&format!("\u{1f4ac} Captain nudge on <b>{title}</b>"))
                        .await;
                }
                Ok(false) => {
                    transition_applied = false;
                }
                Err(e) => {
                    item.status = prev_status;
                    item.intervention_count -= 1;
                    transition_applied = false;
                    tracing::error!(module = "captain", item_id = item.id, error = %e, "persist failed for nudge verdict");
                }
            }

            if transition_applied {
                if let Some(ref w) = item.worker {
                    crate::io::health_store::persist_health_field(
                        w,
                        "pending_ai_feedback",
                        serde_json::json!(verdict.feedback),
                        "failed to persist AI nudge feedback; worker will receive generic template instead",
                    );
                }
                if !inline_resume_worker(item, &verdict.feedback, workflow, pool).await {
                    clear_review_fields = false;
                }
            }
        }
        "respawn" => {
            // Mark old worker session as stopped before clearing refs.
            if let Some(ref sid) = worker_session_id {
                let cwd = item.worktree.as_deref().unwrap_or("");
                if let Err(e) = crate::io::headless_cc::log_session_completion(
                    pool,
                    sid,
                    cwd,
                    "worker",
                    &worker_name,
                    &item.id.to_string(),
                    SessionStatus::Stopped,
                )
                .await
                {
                    warn!(module = "captain", %e, "failed to log session completion on respawn");
                }
            }
            // Snapshot fields that will be cleared, so we can rollback on error.
            let saved_worker_sid = item.session_ids.worker.clone();
            let saved_ask_sid = item.session_ids.ask.clone();
            let saved_worker = item.worker.clone();
            let saved_worktree = item.worktree.clone();
            let saved_branch = item.branch.clone();
            let saved_pr = item.pr.clone();
            let saved_worker_started = item.worker_started_at.clone();

            item.status = ItemStatus::Queued;
            item.session_ids.worker = None;
            item.session_ids.ask = None;
            item.worker = None;
            item.worktree = None;
            item.branch = None;
            item.pr = None;
            item.worker_started_at = None;
            let event = mando_types::timeline::TimelineEvent {
                event_type: TimelineEventType::CaptainReviewVerdict,
                timestamp: mando_types::now_rfc3339(),
                actor: "captain".to_string(),
                summary: format!("Captain respawn: {}", verdict.feedback),
                data,
            };
            match mando_db::queries::tasks::persist_status_transition(
                pool,
                item,
                prev_status.as_str(),
                &event,
            )
            .await
            {
                Ok(true) => {
                    notifier
                        .normal(&format!("\u{1f504} Captain respawning <b>{title}</b>"))
                        .await;
                }
                Ok(false) => {
                    transition_applied = false;
                }
                Err(e) => {
                    // Full rollback of all cleared fields.
                    item.status = prev_status;
                    item.session_ids.worker = saved_worker_sid;
                    item.session_ids.ask = saved_ask_sid;
                    item.worker = saved_worker;
                    item.worktree = saved_worktree;
                    item.branch = saved_branch;
                    item.pr = saved_pr;
                    item.worker_started_at = saved_worker_started;
                    transition_applied = false;
                    tracing::error!(module = "captain", item_id = item.id, error = %e, "persist failed for respawn verdict");
                }
            }
        }
        "escalate" => {
            item.status = ItemStatus::Escalated;
            item.escalation_report = verdict.report.clone();
            let event = mando_types::timeline::TimelineEvent {
                event_type: TimelineEventType::Escalated,
                timestamp: mando_types::now_rfc3339(),
                actor: "captain".to_string(),
                summary: format!("Escalated: {}", verdict.feedback),
                data,
            };
            match mando_db::queries::tasks::persist_status_transition(
                pool,
                item,
                prev_status.as_str(),
                &event,
            )
            .await
            {
                Ok(true) => {
                    notifier
                        .critical(&format!(
                            "\u{1f6a8} Escalated <b>{title}</b>: {}",
                            mando_shared::telegram_format::escape_html(&verdict.feedback),
                        ))
                        .await;
                }
                Ok(false) => {
                    transition_applied = false;
                }
                Err(e) => {
                    item.status = prev_status;
                    item.escalation_report = None;
                    transition_applied = false;
                    tracing::error!(module = "captain", item_id = item.id, error = %e, "persist failed for escalate verdict");
                }
            }
            log_stopped_after = transition_applied;
        }
        "retry_clarifier" => {
            let saved_clarifier_sid = item.session_ids.clarifier.clone();
            let saved_clarifier_fail = item.clarifier_fail_count;
            let saved_worker_started = item.worker_started_at.clone();

            item.status = ItemStatus::New;
            item.session_ids.clarifier = None;
            item.clarifier_fail_count = 0;
            item.worker_started_at = None;
            let event = mando_types::timeline::TimelineEvent {
                event_type: TimelineEventType::CaptainReviewVerdict,
                timestamp: mando_types::now_rfc3339(),
                actor: "captain".to_string(),
                summary: format!("Retry clarifier: {}", verdict.feedback),
                data,
            };
            match mando_db::queries::tasks::persist_status_transition(
                pool,
                item,
                prev_status.as_str(),
                &event,
            )
            .await
            {
                Ok(true) => {
                    notifier
                        .normal(&format!("\u{1f501} Retrying clarifier for <b>{title}</b>"))
                        .await;
                }
                Ok(false) => {
                    transition_applied = false;
                }
                Err(e) => {
                    item.status = prev_status;
                    item.session_ids.clarifier = saved_clarifier_sid;
                    item.clarifier_fail_count = saved_clarifier_fail;
                    item.worker_started_at = saved_worker_started;
                    transition_applied = false;
                    tracing::error!(module = "captain", item_id = item.id, error = %e, "persist failed for retry_clarifier verdict");
                }
            }
            log_stopped_after = transition_applied;
        }
        "reset_budget" => {
            let old_count = item.intervention_count;
            item.intervention_count = 0;
            item.status = ItemStatus::InProgress;
            item.worker_started_at = Some(mando_types::now_rfc3339());
            let event = mando_types::timeline::TimelineEvent {
                event_type: TimelineEventType::CaptainReviewVerdict,
                timestamp: mando_types::now_rfc3339(),
                actor: "captain".to_string(),
                summary: format!(
                    "Captain reset budget ({old_count} -> 0) and nudged: {}",
                    verdict.feedback
                ),
                data,
            };
            match mando_db::queries::tasks::persist_status_transition(
                pool,
                item,
                prev_status.as_str(),
                &event,
            )
            .await
            {
                Ok(true) => {
                    notifier
                        .normal(&format!(
                            "\u{1f504} Captain reset budget on <b>{title}</b> ({old_count} \u{2192} 0)"
                        ))
                        .await;
                }
                Ok(false) => {
                    transition_applied = false;
                }
                Err(e) => {
                    item.status = prev_status;
                    item.intervention_count = old_count;
                    transition_applied = false;
                    tracing::error!(module = "captain", item_id = item.id, error = %e, "persist failed for reset_budget verdict");
                }
            }

            if transition_applied {
                if let Some(ref w) = item.worker {
                    crate::io::health_store::persist_health_field(
                        w,
                        "nudge_reason_consecutive",
                        serde_json::json!(0),
                        "failed to reset nudge circuit breaker after reset_budget",
                    );
                    crate::io::health_store::persist_health_field(
                        w,
                        "last_nudge_reason",
                        serde_json::json!(null),
                        "failed to clear last nudge reason after reset_budget",
                    );
                    crate::io::health_store::persist_health_field(
                        w,
                        "pending_ai_feedback",
                        serde_json::json!(verdict.feedback),
                        "failed to persist AI feedback after reset_budget; worker will receive generic template instead",
                    );
                }
                if !inline_resume_worker(item, &verdict.feedback, workflow, pool).await {
                    clear_review_fields = false;
                }
            }
        }
        other => {
            warn!(module = "captain", action = %other, "unknown verdict action, escalating");
            item.status = ItemStatus::Escalated;
            let event = mando_types::timeline::TimelineEvent {
                event_type: TimelineEventType::Escalated,
                timestamp: mando_types::now_rfc3339(),
                actor: "captain".to_string(),
                summary: format!("Unknown verdict '{other}', escalated"),
                data,
            };
            match mando_db::queries::tasks::persist_status_transition(
                pool,
                item,
                prev_status.as_str(),
                &event,
            )
            .await
            {
                Ok(true) => {
                    notifier
                        .critical(&format!(
                            "\u{1f6a8} Unknown verdict on <b>{title}</b>, escalated"
                        ))
                        .await;
                }
                Ok(false) => {
                    transition_applied = false;
                }
                Err(e) => {
                    item.status = prev_status;
                    transition_applied = false;
                    tracing::error!(module = "captain", item_id = item.id, error = %e, "persist failed for unknown verdict");
                }
            }
            log_stopped_after = transition_applied;
        }
    }

    // Single consolidated log call for every arm that marks the worker
    // session as stopped (ship / escalate / retry_clarifier / unknown).
    if log_stopped_after {
        if let Err(e) = crate::io::headless_cc::log_item_session(
            pool,
            item,
            &worker_name,
            SessionStatus::Stopped,
        )
        .await
        {
            warn!(module = "captain", item_id = item.id, %e, "failed to log stopped worker session");
        }
    }

    if clear_review_fields && transition_applied {
        item.captain_review_trigger = None;
        item.session_ids.review = None;
        item.review_fail_count = 0;
    }

    Ok(())
}

/// Handle review error (CC crashed/timed out).
///
/// Retry up to `max_review_retries`, then mark Errored.
pub async fn handle_review_error(
    item: &mut Task,
    error: &str,
    review_fail_count: &mut u32,
    workflow: &CaptainWorkflow,
    notifier: &Notifier,
    pool: &SqlitePool,
) {
    let prev_status = item.status;
    let saved_trigger = item.captain_review_trigger;
    let saved_review_sid = item.session_ids.review.clone();
    *review_fail_count += 1;
    item.session_ids.review = None;
    let max = workflow.agent.max_review_retries;
    let err_data = serde_json::json!({ "error": error, "fail_count": *review_fail_count });

    if *review_fail_count >= max {
        item.status = ItemStatus::Errored;
        item.captain_review_trigger = None;
        let event = mando_types::timeline::TimelineEvent {
            event_type: TimelineEventType::Errored,
            timestamp: mando_types::now_rfc3339(),
            actor: "captain".to_string(),
            summary: format!(
                "Captain review failed {}/{max} times: {error}",
                review_fail_count
            ),
            data: err_data,
        };
        match mando_db::queries::tasks::persist_status_transition(
            pool,
            item,
            prev_status.as_str(),
            &event,
        )
        .await
        {
            Ok(true) => {
                notifier
                    .critical(&format!(
                        "\u{274c} Captain review failed for <b>{}</b>: {error}",
                        escaped_title(item),
                    ))
                    .await;
            }
            Ok(false) => {
                tracing::info!(
                    module = "captain",
                    item_id = item.id,
                    "review error transition already applied"
                );
            }
            Err(e) => {
                item.status = prev_status;
                item.captain_review_trigger = saved_trigger;
                item.session_ids.review = saved_review_sid;
                *review_fail_count -= 1;
                tracing::error!(module = "captain", item_id = item.id, error = %e, "persist failed for review error");
            }
        }
    } else {
        // Stay in CaptainReviewing -- will retry on next tick.
        // No status transition, so use regular timeline emit.
        warn!(module = "captain", fail_count = *review_fail_count, %max, %error,
            "captain review failed, will retry");
        let _ = timeline_emit::emit_for_task(
            item,
            TimelineEventType::CaptainReviewVerdict,
            &format!("Review attempt {}/{max} failed: {error}", review_fail_count),
            err_data,
            pool,
        )
        .await;
    }
}
