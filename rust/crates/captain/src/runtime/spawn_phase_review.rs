//! CaptainReview action handler — extracted from spawn_phase.rs for file length.

use crate::service::lifecycle;
use crate::{Action, Task};
use anyhow::Result;
use settings::config::settings::Config;
use settings::config::workflow::CaptainWorkflow;

#[tracing::instrument(skip_all)]
pub(crate) async fn handle_captain_review(
    action: &Action,
    items: &mut [Task],
    config: &Config,
    workflow: &CaptainWorkflow,
    notifier: &super::notify::Notifier,
    pool: &sqlx::SqlitePool,
) -> Result<()> {
    let trigger = action.reason.as_deref().unwrap_or("captain_decision");

    trigger_captain_review(action, items, config, workflow, notifier, trigger, pool).await;
    Ok(())
}

#[tracing::instrument(skip_all)]
pub(crate) async fn trigger_captain_review(
    action: &Action,
    items: &mut [Task],
    config: &Config,
    workflow: &CaptainWorkflow,
    notifier: &super::notify::Notifier,
    trigger: &str,
    pool: &sqlx::SqlitePool,
) {
    // Snapshot prior status and trigger so we can roll back cleanly if
    // spawn_review fails. Setting CaptainReviewing on failure would hide
    // the real error from the next tick and leave the item stuck.
    let Some(it) = items
        .iter_mut()
        .find(|it| it.worker.as_deref() == Some(&action.worker))
    else {
        return;
    };
    let prior_status = it.status;
    let prior_trigger = it.captain_review_trigger;
    let prior_review_fail_count = it.review_fail_count;
    let prior_last_activity = it.last_activity_at.clone();

    let trigger_enum: crate::ReviewTrigger = match trigger.parse() {
        Ok(t) => t,
        Err(e) => {
            tracing::error!(
                module = "captain",
                worker = %action.worker,
                trigger = %trigger,
                error = %e,
                "unknown review trigger, cannot dispatch to captain review"
            );
            return;
        }
    };
    let db_status = it.status.as_str().to_string();
    super::action_contract::reset_review_retry(it, trigger_enum);

    if let Err(e) = super::captain_review::spawn_review(
        it,
        trigger,
        Some(&db_status),
        config,
        workflow,
        notifier,
        pool,
    )
    .await
    {
        tracing::error!(
            module = "captain",
            worker = %action.worker,
            error = %e,
            "failed to spawn captain review session; rolling back and incrementing fail counter"
        );
        // Roll back to the prior state (do NOT leave the item pinned at
        // CaptainReviewing with no session ID) and bump the fail counter so
        // the next tick can see the repeated failure.
        lifecycle::restore_status(it, prior_status);
        it.captain_review_trigger = prior_trigger;
        it.last_activity_at = prior_last_activity;
        it.review_fail_count = prior_review_fail_count.saturating_add(1);
        it.session_ids.review = None;
    }
}
