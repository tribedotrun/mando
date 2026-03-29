//! CaptainReview action handler — extracted from spawn_phase.rs for file length.

use anyhow::Result;
use mando_config::settings::Config;
use mando_config::workflow::CaptainWorkflow;
use mando_types::captain::Action;
use mando_types::Task;

use crate::runtime::linear_integration;

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

pub(crate) async fn trigger_captain_review(
    action: &Action,
    items: &mut [Task],
    config: &Config,
    workflow: &CaptainWorkflow,
    notifier: &super::notify::Notifier,
    trigger: &str,
    pool: &sqlx::SqlitePool,
) {
    let Some(it) = items
        .iter_mut()
        .find(|it| it.worker.as_deref() == Some(&action.worker))
    else {
        return;
    };

    if let Err(e) =
        super::captain_review::spawn_review(it, trigger, config, workflow, notifier, pool).await
    {
        tracing::error!(
            module = "captain",
            worker = %action.worker,
            error = %e,
            "failed to spawn captain review session"
        );
        it.status = mando_types::task::ItemStatus::CaptainReviewing;
        it.captain_review_trigger = trigger.parse().ok();
    }

    if let Err(e) = linear_integration::writeback_status(it, config).await {
        tracing::warn!(module = "captain", %e, "Linear status writeback failed");
    }
    if let Err(e) = linear_integration::upsert_workpad(
        it,
        config,
        &format!("Captain reviewing ({})", trigger),
        pool,
    )
    .await
    {
        tracing::warn!(module = "captain", %e, "Linear workpad upsert failed");
    }
}
