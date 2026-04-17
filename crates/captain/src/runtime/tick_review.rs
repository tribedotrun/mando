//! Captain review polling extracted from the main tick loop.

use crate::{ItemStatus, Task};
use settings::config::settings::Config;
use settings::config::workflow::CaptainWorkflow;

use super::{captain_review, credential_rate_limit, notify::Notifier, timeline_emit};

pub(super) async fn poll_reviewing_items(
    items: &mut [Task],
    config: &Config,
    workflow: &CaptainWorkflow,
    notifier: &Notifier,
    pool: &sqlx::SqlitePool,
    rate_limited: bool,
) {
    let review_timeout = workflow.agent.captain_review_timeout_s;
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
                .unwrap_or(crate::ReviewTrigger::Retry);
            item.last_activity_at = Some(global_types::now_rfc3339());
            if let Err(e) = captain_review::spawn_review(
                item,
                trigger.as_str(),
                None, // already CaptainReviewing in DB
                config,
                workflow,
                notifier,
                pool,
            )
            .await
            {
                tracing::warn!(module = "captain", item_id = item.id, error = %e, "spawn_review failed");
                captain_review::handle_review_error(
                    item,
                    &format!("spawn_review failed: {e}"),
                    workflow,
                    notifier,
                    pool,
                )
                .await;
            }
            continue;
        }

        // Detect async CC task crash before checking for a verdict.
        if let Some(error_msg) = captain_review::check_review_failed(item) {
            // Check if this failure was caused by rate limiting — if so,
            // activate cooldown and don't count against the retry budget.
            if let Some(sid) = item.session_ids.review.as_deref() {
                if credential_rate_limit::check_and_activate_from_stream(pool, sid).await {
                    tracing::info!(
                        module = "captain",
                        item_id = item.id,
                        "review failed due to rate limit — not counting against retry budget"
                    );
                    let _ = timeline_emit::emit_rate_limited(item, pool).await;
                    item.session_ids.review = None;
                    continue;
                }
            }

            captain_review::handle_review_error(item, &error_msg, workflow, notifier, pool).await;
            continue;
        }

        if let Some(verdict) = captain_review::check_review(item) {
            if let Err(e) =
                captain_review::apply_verdict(item, &verdict, config, workflow, notifier, pool)
                    .await
            {
                tracing::warn!(module = "captain", item_id = item.id, error = %e,
                    "apply_verdict failed, will retry next tick");
            }
            // On success, apply_verdict clears review fields — item moves on.
            // On failure, review fields are preserved — next tick retries.
            continue;
        }

        let is_timed_out = match item.last_activity_at.as_deref() {
            Some(ts) => match time::OffsetDateTime::parse(
                ts,
                &time::format_description::well_known::Rfc3339,
            ) {
                Ok(entered) => {
                    let elapsed = time::OffsetDateTime::now_utc() - entered;
                    elapsed.whole_seconds() as u64 > review_timeout.as_secs()
                }
                Err(e) => {
                    tracing::warn!(
                        module = "captain",
                        item_id = item.id,
                        last_activity_at = %ts,
                        error = %e,
                        "unparseable last_activity_at on reviewing item; skipping this tick"
                    );
                    continue;
                }
            },
            None => {
                tracing::warn!(
                    module = "captain",
                    item_id = item.id,
                    "reviewing item has no last_activity_at timestamp; skipping this tick"
                );
                continue;
            }
        };

        if is_timed_out {
            // Check if the review session was killed by rate limiting.
            let is_rl = match item.session_ids.review.as_deref() {
                Some(sid) => credential_rate_limit::check_and_activate_from_stream(pool, sid).await,
                None => false,
            };
            if is_rl || rate_limited {
                tracing::info!(
                    module = "captain",
                    item_id = item.id,
                    "review timeout during rate limit — not counting against retry budget"
                );
                let _ = timeline_emit::emit_rate_limited(item, pool).await;
                // Clear session so a fresh one spawns after cooldown.
                item.session_ids.review = None;
                continue;
            }
            captain_review::handle_review_error(
                item,
                "review session timed out without producing a verdict",
                workflow,
                notifier,
                pool,
            )
            .await;
        }
    }
}
