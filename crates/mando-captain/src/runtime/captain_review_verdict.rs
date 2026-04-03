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
            item.status = ItemStatus::InProgress;
            item.intervention_count += 1;
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

            // Store AI-generated feedback so the next nudge_item() call uses it
            // instead of a generic template if the inline resume below fails.
            if let Some(ref w) = item.worker {
                crate::io::health_store::persist_health_field(
                    w,
                    "pending_ai_feedback",
                    serde_json::json!(verdict.feedback),
                    "failed to persist AI nudge feedback — worker will receive generic template instead",
                );
            }

            // Resume the worker process inline so the next tick sees a live
            // process (Rule 2 skip) instead of a dead one (Rule 3 broken).
            if let (Some(w), Some(sid), Some(wt)) =
                (&item.worker, &item.session_ids.worker, &item.worktree)
            {
                let stream_path = mando_config::stream_path_for_session(sid);
                if mando_cc::stream_has_broken_session(&stream_path) {
                    warn!(
                        module = "captain", worker = %w,
                        "nudge verdict skipped resume — stream is broken, next tick will handle"
                    );
                } else {
                    let old_pid = crate::io::pid_lookup::resolve_pid(sid, w).unwrap_or(0);
                    if old_pid > 0 {
                        if let Err(e) = mando_cc::kill_process(old_pid).await {
                            warn!(
                                module = "captain", worker = %w, pid = old_pid, error = %e,
                                "failed to kill old process before verdict resume"
                            );
                        }
                    }
                    let wt_path = mando_config::expand_tilde(wt);
                    let stream_size_before = mando_cc::get_stream_file_size(&stream_path);
                    let env = std::collections::HashMap::new();
                    match crate::io::process_manager::resume_worker_process(
                        w,
                        &verdict.feedback,
                        &wt_path,
                        &workflow.models.worker,
                        sid,
                        &env,
                        workflow.models.fallback.as_deref(),
                    )
                    .await
                    {
                        Ok((pid, _)) => {
                            crate::io::pid_registry::register(sid, pid);
                            let health_path = mando_config::worker_health_path();
                            let mut hstate =
                                crate::io::health_store::load_health_state(&health_path);
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
                            crate::io::headless_cc::log_running_session(
                                pool,
                                sid,
                                &wt_path,
                                "worker",
                                w,
                                &item.best_id(),
                                true,
                            )
                            .await;
                        }
                        Err(e) => {
                            warn!(
                                module = "captain", worker = %w, error = %e,
                                "nudge verdict resume failed — next tick will retry"
                            );
                        }
                    }
                }
            } else {
                warn!(
                    module = "captain",
                    item_id = item.id,
                    "nudge verdict has no worker/session/worktree — next tick will handle"
                );
            }
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
            item.pr = None;
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

    // Clear review fields only after successful application so that a
    // failure leaves the session ID intact for retry on the next tick.
    item.captain_review_trigger = None;
    item.session_ids.review = None;
    item.review_fail_count = 0;

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
