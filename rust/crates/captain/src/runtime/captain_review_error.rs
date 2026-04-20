//! Review error handling extracted from captain_review_verdict.

use tracing::warn;

use crate::{ItemStatus, Task, TimelineEventPayload};
use settings::config::workflow::CaptainWorkflow;

use super::captain_review_helpers::escaped_title;
use super::notify::Notifier;
use super::timeline_emit;
use crate::service::lifecycle;

/// Handle review error (CC crashed/timed out).
///
/// Retry up to `max_review_retries`, then mark Errored.
#[tracing::instrument(skip_all)]
pub async fn handle_review_error(
    item: &mut Task,
    error: &str,
    workflow: &CaptainWorkflow,
    notifier: &Notifier,
    pool: &sqlx::SqlitePool,
) {
    let prev_status = item.status;
    let saved_trigger = item.captain_review_trigger;
    let saved_review_sid = item.session_ids.review.clone();
    item.review_fail_count += 1;
    item.session_ids.review = None;
    let max = workflow.agent.max_review_retries;
    let fail_count = item.review_fail_count;

    if fail_count as u32 >= max {
        if let Err(e) = lifecycle::apply_transition(item, ItemStatus::Errored) {
            tracing::error!(
                module = "captain",
                item_id = item.id,
                error = %e,
                "illegal review error transition"
            );
            item.session_ids.review = saved_review_sid;
            item.review_fail_count -= 1;
            return;
        }
        item.captain_review_trigger = None;
        let event = crate::TimelineEvent {
            timestamp: global_types::now_rfc3339(),
            actor: "captain".to_string(),
            summary: format!("Captain review failed {fail_count}/{max} times: {error}",),
            data: TimelineEventPayload::ReviewErrored {
                error: error.to_string(),
                fail_count,
            },
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
                lifecycle::restore_status(item, prev_status);
                item.captain_review_trigger = saved_trigger;
                item.session_ids.review = saved_review_sid;
                item.review_fail_count -= 1;
                tracing::error!(module = "captain", item_id = item.id, error = %e, "persist failed for review error");
            }
        }
    } else {
        // Stay in CaptainReviewing -- will retry on next tick.
        // No status transition, so use regular timeline emit.
        warn!(module = "captain", fail_count, %max, %error,
            "captain review failed, will retry");
        global_infra::best_effort!(
            timeline_emit::emit_for_task(
                item,
                &format!("Review attempt {fail_count}/{max} failed: {error}"),
                TimelineEventPayload::CaptainReviewRetry {
                    action: "retry".to_string(),
                    feedback: error.to_string(),
                    error: error.to_string(),
                    fail_count,
                },
                pool,
            )
            .await,
            "captain_review_error: timeline_emit::emit_for_task( item, &format!('Review attempt"
        );
    }
}
