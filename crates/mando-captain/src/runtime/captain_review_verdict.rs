//! Verdict application logic extracted from captain_review.

use anyhow::Result;
use tracing::warn;

use mando_config::settings::Config;
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

/// Inline resume of a worker process with feedback. Shared by `nudge` and
/// `reset_budget` verdict handlers. Kills old process, checks for broken
/// stream, resumes with feedback, updates health state and session log.
///
/// Returns `true` if the worker was successfully resumed.
async fn inline_resume_worker(
    item: &Task,
    feedback: &str,
    workflow: &CaptainWorkflow,
    pool: &SqlitePool,
) -> bool {
    let (Some(w), Some(sid), Some(wt)) = (&item.worker, &item.session_ids.worker, &item.worktree)
    else {
        warn!(
            module = "captain",
            item_id = item.id,
            "verdict resume has no worker/session/worktree; next tick will handle"
        );
        return false;
    };

    let stream_path = mando_config::stream_path_for_session(sid);
    if mando_cc::stream_has_broken_session(&stream_path) {
        warn!(
            module = "captain", worker = %w,
            "verdict skipped resume; stream is broken, next tick will handle"
        );
        return false;
    }

    let old_pid = crate::io::pid_lookup::resolve_pid(sid, w).unwrap_or(mando_types::Pid::new(0));
    if old_pid.as_u32() > 0 {
        if let Err(e) = mando_cc::kill_process(old_pid).await {
            warn!(
                module = "captain", worker = %w, pid = %old_pid, error = %e,
                "failed to kill old process before verdict resume"
            );
        }
    }

    let wt_path = mando_config::expand_tilde(wt);
    let stream_size_before = mando_cc::get_stream_file_size(&stream_path);
    let env = std::collections::HashMap::new();
    match crate::io::process_manager::resume_worker_process(
        feedback,
        &wt_path,
        &workflow.models.worker,
        sid,
        &env,
        workflow.models.fallback.as_deref(),
    )
    .await
    {
        Ok((pid, _)) => {
            if let Err(e) = crate::io::pid_registry::register(sid, pid) {
                warn!(module = "captain", worker = %w, %e, "pid_registry register failed");
            }
            // Health-state bookkeeping must not abort: the worker is already
            // running. Degrade gracefully on failure instead of double-resuming.
            let health_path = mando_config::worker_health_path();
            match crate::io::health_store::load_health_state(&health_path) {
                Ok(mut hstate) => {
                    crate::io::health_store::set_health_field(
                        &mut hstate,
                        w,
                        "pid",
                        serde_json::json!(pid),
                    );
                    crate::io::health_store::set_health_field(
                        &mut hstate,
                        w,
                        "stream_size_at_spawn",
                        serde_json::json!(stream_size_before),
                    );
                    if let Err(e) =
                        crate::io::health_store::save_health_state(&health_path, &hstate)
                    {
                        warn!(module = "captain", worker = %w, error = %e,
                            "failed to persist health after verdict resume");
                    }
                }
                Err(e) => {
                    warn!(module = "captain", worker = %w, error = %e,
                        "failed to load health state after verdict resume; skipping bookkeeping");
                }
            }
            if let Err(e) = crate::io::headless_cc::log_running_session(
                pool,
                sid,
                &wt_path,
                "worker",
                w,
                &item.id.to_string(),
                true,
            )
            .await
            {
                warn!(module = "captain", worker = %w, %e,
                    "failed to log running session after verdict resume");
            }
            true
        }
        Err(e) => {
            warn!(module = "captain", worker = %w, error = %e,
                "verdict resume failed; next tick will retry");
            false
        }
    }
}

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

    // Outcome tracking. log_stopped_after tells the post-match block to mark
    // the worker session stopped (covers ship/escalate/retry_clarifier/other).
    // clear_review_fields is false only when nudge resume fails, so the next
    // tick can retry the same verdict.
    let mut log_stopped_after = false;
    let mut clear_review_fields = true;

    match verdict.action.as_str() {
        "ship" => {
            let is_no_pr = item.no_pr;
            item.status = spawn_logic::ship_status(is_no_pr);
            let (event, msg_suffix) = if is_no_pr {
                (TimelineEventType::CompletedNoPr, "completed (no PR)")
            } else {
                (TimelineEventType::AwaitingReview, "ready for review")
            };
            let _ = timeline_emit::emit_for_task(
                item,
                event,
                &format!("Captain approved; {msg_suffix}"),
                data,
                pool,
            )
            .await;
            notifier
                .high(&format!(
                    "\u{2705} Captain approved <b>{title}</b>; {msg_suffix}"
                ))
                .await;
            log_stopped_after = true;
        }
        "nudge" => {
            item.status = ItemStatus::InProgress;
            item.intervention_count += 1;
            // Reset timeout clock so the worker gets a fresh window after review.
            item.worker_started_at = Some(mando_types::now_rfc3339());
            let _ = timeline_emit::emit_for_task(
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

            // Store AI-generated feedback so the next nudge_item() call uses it
            // instead of a generic template if the inline resume below fails.
            if let Some(ref w) = item.worker {
                crate::io::health_store::persist_health_field(
                    w,
                    "pending_ai_feedback",
                    serde_json::json!(verdict.feedback),
                    "failed to persist AI nudge feedback; worker will receive generic template instead",
                );
            }

            // Resume the worker process inline so the next tick sees a live
            // process (Rule 2 skip) instead of a dead one (Rule 3 broken).
            // clear_review_fields stays false until we confirm the resume
            // succeeded; failed resumes leave the verdict session ID intact
            // so the next tick can retry.
            if !inline_resume_worker(item, &verdict.feedback, workflow, pool).await {
                clear_review_fields = false;
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
            item.status = ItemStatus::Queued;
            item.session_ids.worker = None;
            item.session_ids.ask = None;
            item.worker = None;
            item.worktree = None;
            item.branch = None;
            item.pr = None;
            item.worker_started_at = None;
            let _ = timeline_emit::emit_for_task(
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
            let _ = timeline_emit::emit_for_task(
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
            log_stopped_after = true;
        }
        "retry_clarifier" => {
            item.status = ItemStatus::Clarifying;
            item.worker_started_at = None;
            let _ = timeline_emit::emit_for_task(
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
            log_stopped_after = true;
        }
        "reset_budget" => {
            // Captain overrides the intervention budget and nudges the worker
            // with fresh instructions. This is the captain's authority to
            // unblock tasks that exhausted their budget due to false alarms
            // or recoverable issues.
            let old_count = item.intervention_count;
            item.intervention_count = 0;
            item.status = ItemStatus::InProgress;
            item.worker_started_at = Some(mando_types::now_rfc3339());
            let _ = timeline_emit::emit_for_task(
                item,
                TimelineEventType::CaptainReviewVerdict,
                &format!(
                    "Captain reset budget ({old_count} -> 0) and nudged: {}",
                    verdict.feedback
                ),
                data,
                pool,
            )
            .await;
            notifier
                .normal(&format!(
                    "\u{1f504} Captain reset budget on <b>{title}</b> ({old_count} \u{2192} 0)"
                ))
                .await;

            // Clear repeated-nudge circuit breaker so the fresh budget
            // doesn't immediately re-trigger a review loop.
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
        other => {
            warn!(module = "captain", action = %other, "unknown verdict action, escalating");
            item.status = ItemStatus::Escalated;
            let _ = timeline_emit::emit_for_task(
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
            log_stopped_after = true;
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

    if clear_review_fields {
        // Clear review fields only after successful application so that a
        // failed nudge resume leaves the session ID intact for retry on the
        // next tick.
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
    *review_fail_count += 1;
    item.session_ids.review = None;
    let max = workflow.agent.max_review_retries;
    let err_data = serde_json::json!({ "error": error, "fail_count": *review_fail_count });

    if *review_fail_count >= max {
        item.status = ItemStatus::Errored;
        item.captain_review_trigger = None;
        let _ = timeline_emit::emit_for_task(
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
