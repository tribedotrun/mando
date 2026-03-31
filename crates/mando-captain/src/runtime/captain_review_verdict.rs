//! Verdict application logic extracted from captain_review.

use anyhow::Result;
use tracing::warn;

use mando_config::workflow::CaptainWorkflow;
use mando_types::task::{ItemStatus, Task};
use mando_types::timeline::TimelineEventType;
use mando_types::SessionStatus;

use sqlx::SqlitePool;

use super::notify::Notifier;
use super::timeline_emit;
use crate::biz::spawn_logic;

pub(crate) fn escaped_title(item: &Task) -> String {
    mando_shared::telegram_format::escape_html(&item.title)
}

/// Apply a captain review verdict to an item.
pub async fn apply_verdict(
    item: &mut Task,
    verdict: &super::captain_review::CaptainVerdict,
    notifier: &Notifier,
    pool: &SqlitePool,
) -> Result<()> {
    item.captain_review_trigger = None;
    item.session_ids.review = None;
    item.review_fail_count = 0;

    // Capture worker info before any arm clears it (respawn sets these to None).
    let worker_session_id = item.session_ids.worker.clone();
    let worker_name = item.worker.clone().unwrap_or_default();

    let data = serde_json::json!({ "action": verdict.action, "feedback": verdict.feedback });
    let title = escaped_title(item);

    match verdict.action.as_str() {
        "ship" => {
            let is_no_pr = item.no_pr;
            item.status = spawn_logic::ship_status(is_no_pr);
            let (event, msg_suffix) = if is_no_pr {
                (TimelineEventType::CompletedNoPr, "completed (no PR)")
            } else {
                (TimelineEventType::AwaitingReview, "ready for review")
            };
            timeline_emit::emit_for_task(
                item,
                event,
                &format!("Captain approved — {msg_suffix}"),
                data,
                pool,
            )
            .await;
            notifier
                .high(&format!(
                    "\u{2705} Captain approved <b>{title}</b> — {msg_suffix}"
                ))
                .await;
            // Mark worker session as stopped.
            crate::io::headless_cc::log_item_session(
                pool,
                item,
                &worker_name,
                SessionStatus::Stopped,
            )
            .await;
        }
        "nudge" => {
            // Don't update session status — worker will be resumed on next tick,
            // which will call log_running_session.
            item.status = ItemStatus::InProgress;
            timeline_emit::emit_for_task(
                item,
                TimelineEventType::CaptainReviewVerdict,
                &format!("Captain nudge: {}", verdict.feedback),
                data,
                pool,
            )
            .await;
            notifier
                .normal(&format!("\u{1f4ac} Captain nudge on <b>{title}</b>"))
                .await;
        }
        "respawn" => {
            // Mark old worker session as stopped before clearing refs.
            if let Some(ref sid) = worker_session_id {
                let cwd = item.worktree.as_deref().unwrap_or("");
                crate::io::headless_cc::log_session_completion(
                    pool,
                    sid,
                    cwd,
                    "worker",
                    &worker_name,
                    &item.best_id(),
                    SessionStatus::Stopped,
                )
                .await;
            }
            item.status = ItemStatus::Queued;
            item.session_ids.worker = None;
            item.worker = None;
            item.worktree = None;
            item.branch = None;
            timeline_emit::emit_for_task(
                item,
                TimelineEventType::CaptainReviewVerdict,
                &format!("Captain respawn: {}", verdict.feedback),
                data,
                pool,
            )
            .await;
            notifier
                .normal(&format!("\u{1f504} Captain respawning <b>{title}</b>"))
                .await;
        }
        "escalate" => {
            item.status = ItemStatus::Escalated;
            item.escalation_report = verdict.report.clone();
            timeline_emit::emit_for_task(
                item,
                TimelineEventType::Escalated,
                &format!("Escalated: {}", verdict.feedback),
                data,
                pool,
            )
            .await;
            notifier
                .critical(&format!(
                    "\u{1f6a8} Escalated <b>{title}</b>: {}",
                    mando_shared::telegram_format::escape_html(&verdict.feedback),
                ))
                .await;
            // Mark worker session as stopped.
            crate::io::headless_cc::log_item_session(
                pool,
                item,
                &worker_name,
                SessionStatus::Stopped,
            )
            .await;
        }
        "retry_clarifier" => {
            item.status = ItemStatus::Clarifying;
            timeline_emit::emit_for_task(
                item,
                TimelineEventType::CaptainReviewVerdict,
                &format!("Retry clarifier: {}", verdict.feedback),
                data,
                pool,
            )
            .await;
            notifier
                .normal(&format!("\u{1f501} Retrying clarifier for <b>{title}</b>"))
                .await;
            // Mark worker session as stopped.
            crate::io::headless_cc::log_item_session(
                pool,
                item,
                &worker_name,
                SessionStatus::Stopped,
            )
            .await;
        }
        other => {
            warn!(module = "captain", action = %other, "unknown verdict action, escalating");
            item.status = ItemStatus::Escalated;
            timeline_emit::emit_for_task(
                item,
                TimelineEventType::Escalated,
                &format!("Unknown verdict '{other}', escalated"),
                data,
                pool,
            )
            .await;
            notifier
                .critical(&format!(
                    "\u{1f6a8} Unknown verdict on <b>{title}</b>, escalated"
                ))
                .await;
            // Mark worker session as stopped.
            crate::io::headless_cc::log_item_session(
                pool,
                item,
                &worker_name,
                SessionStatus::Stopped,
            )
            .await;
        }
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
    *review_fail_count += 1;
    item.session_ids.review = None;
    let max = workflow.agent.max_review_retries;
    let err_data = serde_json::json!({ "error": error, "fail_count": *review_fail_count });

    if *review_fail_count >= max {
        item.status = ItemStatus::Errored;
        item.captain_review_trigger = None;
        timeline_emit::emit_for_task(
            item,
            TimelineEventType::Errored,
            &format!(
                "Captain review failed {}/{max} times: {error}",
                review_fail_count
            ),
            err_data,
            pool,
        )
        .await;
        notifier
            .critical(&format!(
                "\u{274c} Captain review failed for <b>{}</b>: {error}",
                escaped_title(item),
            ))
            .await;
    } else {
        warn!(module = "captain", fail_count = *review_fail_count, %max, %error,
            "captain review failed, will retry");
        timeline_emit::emit_for_task(
            item,
            TimelineEventType::CaptainReviewVerdict,
            &format!("Review attempt {}/{max} failed: {error}", review_fail_count),
            err_data,
            pool,
        )
        .await;
    }
}
