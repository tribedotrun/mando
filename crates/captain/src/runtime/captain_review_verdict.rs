//! Verdict application logic extracted from captain_review.

use anyhow::Result;
use tracing::warn;

use crate::{ItemStatus, Task, TimelineEventType};
use global_types::SessionStatus;
use settings::config::settings::Config;
use settings::config::workflow::CaptainWorkflow;

use sqlx::SqlitePool;

use super::captain_review_helpers::{escaped_title, inline_resume_worker};
use super::notify::Notifier;
use crate::service::spawn_logic;

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

    // On a ship verdict, capture the git HEAD SHA of the worktree that was
    // just reviewed. The mergeability tick compares this against the current
    // PR head before auto-merging so that commits pushed after review
    // (e.g. by a rebase worker) force a human re-review instead of being
    // auto-merged on a stale high-confidence verdict.
    let reviewed_head_sha = if verdict.action == "ship" {
        read_worktree_head_sha(item.worktree.as_deref()).await
    } else {
        None
    };

    // Timeline event data carries the full verdict including confidence
    // (populated only on ship). Mergeability tick reads `confidence` from
    // the latest CaptainReviewVerdict event to decide auto-merge.
    let mut data_obj = serde_json::Map::new();
    data_obj.insert(
        "action".into(),
        serde_json::Value::String(verdict.action.clone()),
    );
    data_obj.insert(
        "feedback".into(),
        serde_json::Value::String(verdict.feedback.clone()),
    );
    if let Some(ref c) = verdict.confidence {
        data_obj.insert("confidence".into(), serde_json::Value::String(c.clone()));
    }
    if let Some(ref r) = verdict.confidence_reason {
        data_obj.insert(
            "confidence_reason".into(),
            serde_json::Value::String(r.clone()),
        );
    }
    if let Some(ref sha) = reviewed_head_sha {
        data_obj.insert(
            "reviewed_head_sha".into(),
            serde_json::Value::String(sha.clone()),
        );
    }
    let data = serde_json::Value::Object(data_obj);
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
            let old_intervention_count = item.intervention_count;
            item.status = spawn_logic::ship_status(is_no_pr);
            item.intervention_count = 0;
            let (event_type, msg_suffix) = if is_no_pr {
                (TimelineEventType::CompletedNoPr, "completed (no PR)")
            } else {
                (TimelineEventType::AwaitingReview, "ready for review")
            };
            // Summary carries confidence so humans see it inline in the
            // timeline feed without expanding the event's data blob.
            let confidence_badge = verdict
                .confidence
                .as_deref()
                .map(|c| format!(" (confidence: {c})"))
                .unwrap_or_default();
            let event = crate::TimelineEvent {
                event_type,
                timestamp: global_types::now_rfc3339(),
                actor: "captain".to_string(),
                summary: format!("Captain approved{confidence_badge}; {msg_suffix}"),
                data,
            };
            match crate::io::queries::tasks::persist_status_transition(
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
                            "\u{2705} Captain approved <b>{title}</b>{confidence_badge}; {msg_suffix}"
                        ))
                        .await;
                }
                Ok(false) => {
                    transition_applied = false;
                }
                Err(e) => {
                    item.status = prev_status;
                    item.intervention_count = old_intervention_count;
                    transition_applied = false;
                    tracing::error!(module = "captain", item_id = item.id, error = %e, "persist failed for ship verdict");
                }
            }
            log_stopped_after = transition_applied;
        }
        "nudge" => {
            item.status = ItemStatus::InProgress;
            item.intervention_count += 1;
            item.worker_started_at = Some(global_types::now_rfc3339());
            let event = crate::TimelineEvent {
                event_type: TimelineEventType::CaptainReviewVerdict,
                timestamp: global_types::now_rfc3339(),
                actor: "captain".to_string(),
                summary: format!("Captain nudge: {}", verdict.feedback),
                data,
            };
            match crate::io::queries::tasks::persist_status_transition(
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
                    Some(item.id),
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
            let saved_pr = item.pr_number;
            let saved_worker_started = item.worker_started_at.clone();

            item.status = ItemStatus::Queued;
            item.session_ids.worker = None;
            item.session_ids.ask = None;
            item.worker = None;
            item.worktree = None;
            // workbench_id is permanent — once assigned, never cleared.
            item.branch = None;
            item.pr_number = None;
            item.worker_started_at = None;
            let event = crate::TimelineEvent {
                event_type: TimelineEventType::CaptainReviewVerdict,
                timestamp: global_types::now_rfc3339(),
                actor: "captain".to_string(),
                summary: format!("Captain respawn: {}", verdict.feedback),
                data,
            };
            match crate::io::queries::tasks::persist_status_transition(
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
                    item.pr_number = saved_pr;
                    item.worker_started_at = saved_worker_started;
                    transition_applied = false;
                    tracing::error!(module = "captain", item_id = item.id, error = %e, "persist failed for respawn verdict");
                }
            }
        }
        "escalate" => {
            item.status = ItemStatus::Escalated;
            item.escalation_report = verdict.report.clone();
            let event = crate::TimelineEvent {
                event_type: TimelineEventType::Escalated,
                timestamp: global_types::now_rfc3339(),
                actor: "captain".to_string(),
                summary: format!("Escalated: {}", verdict.feedback),
                data,
            };
            match crate::io::queries::tasks::persist_status_transition(
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
                            global_infra::html::escape_html(&verdict.feedback),
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
            let event = crate::TimelineEvent {
                event_type: TimelineEventType::CaptainReviewVerdict,
                timestamp: global_types::now_rfc3339(),
                actor: "captain".to_string(),
                summary: format!("Retry clarifier: {}", verdict.feedback),
                data,
            };
            match crate::io::queries::tasks::persist_status_transition(
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
            item.worker_started_at = Some(global_types::now_rfc3339());
            let event = crate::TimelineEvent {
                event_type: TimelineEventType::CaptainReviewVerdict,
                timestamp: global_types::now_rfc3339(),
                actor: "captain".to_string(),
                summary: format!(
                    "Captain reset budget ({old_count} -> 0) and nudged: {}",
                    verdict.feedback
                ),
                data,
            };
            match crate::io::queries::tasks::persist_status_transition(
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
            let event = crate::TimelineEvent {
                event_type: TimelineEventType::Escalated,
                timestamp: global_types::now_rfc3339(),
                actor: "captain".to_string(),
                summary: format!("Unknown verdict '{other}', escalated"),
                data,
            };
            match crate::io::queries::tasks::persist_status_transition(
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

/// Read `git rev-parse HEAD` from a worktree. Returns `None` if the worktree
/// path is missing, the command errors, or the output isn't a valid SHA-1 or
/// SHA-256 hex string. Used to stamp ship verdicts with the reviewed head so
/// the mergeability tick can detect post-review pushes.
async fn read_worktree_head_sha(worktree: Option<&str>) -> Option<String> {
    let wt = worktree?;
    let output = tokio::process::Command::new("git")
        .args(["-C", wt, "rev-parse", "HEAD"])
        .output()
        .await
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let sha = String::from_utf8(output.stdout).ok()?.trim().to_string();
    if sha.is_empty() || !sha.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    Some(sha)
}
