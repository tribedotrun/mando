//! Spawn phase — execute captain actions (nudge, captain-review).

use crate::{Action, ActionKind, Task};
use anyhow::Result;
use settings::Config;

use settings::CaptainWorkflow;

/// Execute a single captain action against the live item list.
#[tracing::instrument(skip_all, fields(module = "captain", action_kind = ?action.action, worker = %action.worker))]
pub(crate) async fn execute_action(
    action: &Action,
    items: &mut [Task],
    config: &Config,
    workflow: &CaptainWorkflow,
    notifier: &super::notify::Notifier,
    alerts: &mut Vec<String>,
    pool: &sqlx::SqlitePool,
) -> Result<()> {
    match action.action {
        ActionKind::Skip => {}
        ActionKind::Nudge => {
            execute_nudge(action, items, config, workflow, notifier, alerts, pool).await?;
        }
        ActionKind::CaptainReview => {
            super::spawn_phase_review::handle_captain_review(
                action, items, config, workflow, notifier, pool,
            )
            .await?;
        }
    }
    Ok(())
}

/// Nudge: check budget, kill old process, resume with message.
async fn execute_nudge(
    action: &Action,
    items: &mut [Task],
    config: &Config,
    workflow: &CaptainWorkflow,
    notifier: &super::notify::Notifier,
    alerts: &mut Vec<String>,
    pool: &sqlx::SqlitePool,
) -> Result<()> {
    let Some(item) = items
        .iter_mut()
        .find(|it| it.worker.as_deref() == Some(&action.worker))
    else {
        return Ok(());
    };
    super::action_contract::nudge_item(
        item,
        action.message.as_deref(),
        action.reason.as_deref(),
        config,
        workflow,
        notifier,
        alerts,
        pool,
    )
    .await
}
