//! Captain review polling extracted from the main tick loop.

use mando_config::settings::Config;
use mando_config::workflow::CaptainWorkflow;
use mando_types::task::ItemStatus;
use mando_types::Task;

use super::{captain_review, notify::Notifier, rate_limit_cooldown};

pub(super) async fn poll_reviewing_items(
    items: &mut [Task],
    config: &Config,
    workflow: &CaptainWorkflow,
    notifier: &Notifier,
    pool: &sqlx::SqlitePool,
    rate_limited: bool,
) {
    let review_timeout_s = workflow.agent.captain_review_timeout_s;
    for item in items
        .iter_mut()
        .filter(|it| it.status == ItemStatus::CaptainReviewing)
    {
        let has_session = item
            .session_ids
            .review
            .as_deref()
            .is_some_and(|s| !s.is_empty());
        if !has_session {
            // During rate-limit cooldown, skip spawning — will retry after cooldown.
            if rate_limited {
                tracing::debug!(
                    module = "captain",
                    item_id = item.id,
                    "skipping review spawn during rate-limit cooldown"
                );
                continue;
            }
            let trigger = item
                .captain_review_trigger
                .unwrap_or(mando_types::task::ReviewTrigger::Retry);
            item.last_activity_at = Some(mando_types::now_rfc3339());
            if let Err(e) = captain_review::spawn_review(
                item,
                trigger.as_str(),
                config,
                workflow,
                notifier,
                pool,
            )
            .await
            {
                tracing::warn!(module = "captain", item_id = item.id, error = %e, "spawn_review failed");
                // Increment review_fail_count and move to Errored if budget is
                // exhausted so the item does not get stuck retrying forever.
                let mut fail_count = item.review_fail_count as u32;
                captain_review::handle_review_error(
                    item,
                    &format!("spawn_review failed: {e}"),
                    &mut fail_count,
                    workflow,
                    notifier,
                    pool,
                )
                .await;
                item.review_fail_count = fail_count as i64;
            }
            continue;
        }

        // Detect async CC task crash before checking for a verdict.
        if let Some(error_msg) = captain_review::check_review_failed(item) {
            // Check if this failure was caused by rate limiting — if so,
            // activate cooldown and don't count against the retry budget.
            let is_rl = item
                .session_ids
                .review
                .as_deref()
                .is_some_and(rate_limit_cooldown::check_and_activate_from_stream);
            if is_rl {
                tracing::info!(
                    module = "captain",
                    item_id = item.id,
                    "review failed due to rate limit — not counting against retry budget"
                );
                item.session_ids.review = None;
                continue;
            }

            let mut fail_count = item.review_fail_count as u32;
            captain_review::handle_review_error(
                item,
                &error_msg,
                &mut fail_count,
                workflow,
                notifier,
                pool,
            )
            .await;
            item.review_fail_count = fail_count as i64;
            continue;
        }

        if let Some(verdict) = captain_review::check_review(item) {
            if let Err(e) = captain_review::apply_verdict(item, &verdict, notifier, pool).await {
                tracing::warn!(module = "captain", item_id = item.id, error = %e,
                    "apply_verdict failed, will retry next tick");
            }
            // On success, apply_verdict clears review fields — item moves on.
            // On failure, review fields are preserved — next tick retries.
            continue;
        }

        let is_timed_out = item
            .last_activity_at
            .as_deref()
            .and_then(|ts| {
                time::OffsetDateTime::parse(ts, &time::format_description::well_known::Rfc3339).ok()
            })
            .map(|entered| {
                let elapsed = time::OffsetDateTime::now_utc() - entered;
                elapsed.whole_seconds() as u64 > review_timeout_s
            })
            .unwrap_or(true);

        if is_timed_out {
            // Check if the review session was killed by rate limiting.
            let is_rl = item
                .session_ids
                .review
                .as_deref()
                .is_some_and(rate_limit_cooldown::check_and_activate_from_stream);
            if is_rl || rate_limited {
                tracing::info!(
                    module = "captain",
                    item_id = item.id,
                    "review timeout during rate limit — not counting against retry budget"
                );
                // Clear session so a fresh one spawns after cooldown.
                item.session_ids.review = None;
                continue;
            }
            let mut fail_count = item.review_fail_count as u32;
            captain_review::handle_review_error(
                item,
                "review session timed out without producing a verdict",
                &mut fail_count,
                workflow,
                notifier,
                pool,
            )
            .await;
            item.review_fail_count = fail_count as i64;
        }
    }
}
