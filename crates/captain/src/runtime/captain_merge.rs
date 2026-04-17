//! Async, non-blocking captain merge sessions.
//!
//! When an item enters CaptainMerging, the captain:
//! 1. Spawns a headless CC session with merge instructions
//! 2. The session checks CI, triggers it if needed, fixes failures, and merges
//! 3. On subsequent ticks, polls for completion
//! 4. Applies the result (merged or failed → retry)

use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::{ItemStatus, Task, TimelineEventType};
use settings::config::workflow::CaptainWorkflow;

use super::notify::Notifier;
use super::timeline_emit;

pub(crate) use super::captain_merge_spawn::spawn_merge;

/// Structured result from a captain merge CC session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeResult {
    pub action: String,
    pub feedback: String,
}

/// JSON Schema for the MergeResult structured output.
pub(super) fn merge_json_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "action": {
                "type": "string",
                "enum": ["merged"],
                "description": "merged = PR successfully merged"
            },
            "feedback": {
                "type": "string",
                "description": "Summary of what was done to merge the PR"
            }
        },
        "required": ["action", "feedback"]
    })
}

/// Check if a captain merge session has completed. Returns the result if done.
pub(crate) fn check_merge(item: &Task) -> Option<MergeResult> {
    let session_id = item.session_ids.merge.as_deref()?;
    let stream_path = global_infra::paths::stream_path_for_session(session_id);
    let result = match global_claude::get_stream_result(&stream_path) {
        Some(r) => r,
        None => {
            let stream_size = std::fs::metadata(&stream_path)
                .map(|m| m.len())
                .unwrap_or(u64::MAX);
            tracing::debug!(
                module = "captain",
                item_id = item.id,
                %session_id,
                stream_file_bytes = stream_size,
                stream_path = %stream_path.display(),
                "check_merge: no result in stream file"
            );
            return None;
        }
    };

    // Try structured_output first.
    if let Some(so) = result.get("structured_output").filter(|v| !v.is_null()) {
        match serde_json::from_value::<MergeResult>(so.clone()) {
            Ok(mr) => return Some(mr),
            Err(e) => {
                warn!(module = "captain", %e, %session_id, "merge structured_output parse failed");
            }
        }
    }

    // Fall back to result text.
    let mut text = result
        .get("result")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    if text.is_empty() {
        if let Some(t) = global_claude::get_last_assistant_text(&stream_path) {
            text = t;
        } else {
            return Some(MergeResult {
                action: "failed".into(),
                feedback: "Merge session completed but produced no output".into(),
            });
        }
    }

    match serde_json::from_str::<MergeResult>(&text) {
        Ok(mr) => Some(mr),
        Err(e) => {
            warn!(module = "captain", %e, "failed to parse merge result");
            Some(MergeResult {
                action: "failed".into(),
                feedback: format!("Failed to parse merge result: {e}"),
            })
        }
    }
}

/// Apply a merge session result to an item.
///
/// "merged" → terminal success. Any other action (including "failed") is
/// treated as a retryable failure and routed through `handle_merge_error`,
/// which retries up to `max_merge_retries` before routing to
/// CaptainReviewing (invariant: Escalated only via CaptainReviewing verdict).
pub(crate) async fn apply_merge_result(
    item: &mut Task,
    result: &MergeResult,
    notifier: &Notifier,
    workflow: &CaptainWorkflow,
    pool: &sqlx::SqlitePool,
) {
    let title = global_infra::html::escape_html(&item.title);
    let data = serde_json::json!({ "action": result.action, "feedback": result.feedback });
    let prev_status = item.status;

    // Mutate in-memory state first (persist_status_transition reads from the task).
    item.session_ids.merge = None;

    match result.action.as_str() {
        "merged" => {
            item.status = ItemStatus::Merged;
            item.merge_fail_count = 0;

            let event = crate::TimelineEvent {
                event_type: TimelineEventType::Merged,
                timestamp: global_types::now_rfc3339(),
                actor: "captain".to_string(),
                summary: format!("Captain merged: {}", result.feedback),
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
                        .high(&format!("\u{1f389} Captain merged <b>{title}</b>"))
                        .await;
                }
                Ok(false) => {
                    tracing::info!(
                        module = "captain",
                        item_id = item.id,
                        "merge result already applied, skipping"
                    );
                }
                Err(e) => {
                    // Rollback in-memory state so the tick can retry.
                    item.status = prev_status;
                    item.session_ids.merge = None; // keep cleared — session is done
                    tracing::error!(module = "captain", item_id = item.id, error = %e, "persist_status_transition failed for merge");
                }
            }
        }
        _ => {
            // Non-merged result → treat as retryable failure. Route through
            // handle_merge_error which retries up to max_merge_retries before
            // routing to CaptainReviewing (never directly to Escalated).
            let max_retries = workflow.agent.max_merge_retries;
            handle_merge_error(item, &result.feedback, max_retries, notifier, pool).await;
        }
    }
}

/// Handle merge session error (CC crashed/timed out).
///
/// Retries up to `max_merge_retries` before routing to CaptainReviewing —
/// transient failures (GitHub API blips, CC timeouts) are common during merge
/// operations. When retries are exhausted, routes through CaptainReviewing
/// with a merge_fail trigger (invariant 1: Escalated only via CaptainReviewing).
pub(crate) async fn handle_merge_error(
    item: &mut Task,
    error: &str,
    max_retries: u32,
    notifier: &Notifier,
    pool: &sqlx::SqlitePool,
) {
    let prev_status = item.status;
    item.session_ids.merge = None;
    item.merge_fail_count += 1;
    let fail_count = item.merge_fail_count as u32;
    let title = global_infra::html::escape_html(&item.title);
    let err_data = serde_json::json!({ "error": error, "fail_count": fail_count });

    if fail_count >= max_retries {
        // Build enriched report with actionable context.
        let pr_ref = item
            .pr_number
            .map(crate::pr_label)
            .unwrap_or_else(|| "unknown".to_string());
        let has_conflicts = item.rebase_worker.as_deref().is_some_and(|w| w == "failed");
        let report = format!(
            "## Merge failure report\n\
             \n\
             - **PR:** {pr_ref}\n\
             - **Error:** {error}\n\
             - **Attempts:** {fail_count}/{max_retries}\n\
             - **Conflicts detected:** {has_conflicts}\n\
             - **Merge fail count:** {fail_count}",
        );

        // Snapshot before reset_review_retry so we can roll back on Err.
        let snap = super::action_contract::ReviewFieldsSnapshot::capture(item);
        let saved_escalation = item.escalation_report.clone();

        // Route through CaptainReviewing (merge_fail trigger) instead of
        // escalating directly — invariant 1.
        super::action_contract::reset_review_retry(item, crate::ReviewTrigger::MergeFail);
        item.escalation_report = Some(report);

        let event = crate::TimelineEvent {
            event_type: TimelineEventType::CaptainReviewStarted,
            timestamp: global_types::now_rfc3339(),
            actor: "captain".to_string(),
            summary: format!(
                "Merge failed {fail_count}/{max_retries} — captain reviewing: {error}"
            ),
            data: err_data,
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
                let escaped_error = global_infra::html::escape_html(error);
                notifier
                    .critical(&format!(
                        "\u{1f6a8} Merge failed for <b>{title}</b>: {escaped_error} — captain reviewing"
                    ))
                    .await;
            }
            Ok(false) => {
                tracing::info!(
                    module = "captain",
                    item_id = item.id,
                    "merge error transition already applied"
                );
            }
            Err(e) => {
                snap.restore(item);
                item.escalation_report = saved_escalation;
                tracing::error!(module = "captain", item_id = item.id, error = %e, "persist_status_transition failed for merge error");
            }
        }
    } else {
        // Stay in CaptainMerging — will retry on next tick.
        // This is a retry within the same status, so use regular timeline emit
        // (no status transition to guard).
        tracing::warn!(module = "captain", fail_count, max = max_retries, %error,
            "merge session failed, will retry");
        let _ = timeline_emit::emit_for_task(
            item,
            TimelineEventType::CaptainMergeStarted,
            &format!("Merge attempt {fail_count}/{max_retries} failed: {error}"),
            err_data,
            pool,
        )
        .await;
    }
}
