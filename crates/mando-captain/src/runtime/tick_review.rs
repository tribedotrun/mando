//! Captain review polling extracted from the main tick loop.

use mando_config::settings::Config;
use mando_config::workflow::CaptainWorkflow;
use mando_types::task::ItemStatus;
use mando_types::Task;

use super::{captain_review, notify::Notifier};

pub(super) async fn poll_reviewing_items(
    items: &mut [Task],
    config: &Config,
    workflow: &CaptainWorkflow,
    notifier: &Notifier,
    pool: &sqlx::SqlitePool,
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
            }
            continue;
        }

        if let Some(verdict) = captain_review::check_review(item) {
            if let Err(e) = captain_review::apply_verdict(item, &verdict, notifier, pool).await {
                tracing::warn!(module = "captain", item_id = item.id, error = %e, "apply_verdict failed");
            }
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
