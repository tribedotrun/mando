use anyhow::{Context, Result};
use settings::CaptainWorkflow;

use crate::io::session_terminate;
use crate::service::lifecycle;
use crate::{ItemStatus, ReviewTrigger, Task};

use super::captain_review_helpers::{
    notify_review_started, review_guard_status, review_started_event, rollback_review_spawn,
};
use super::notify::Notifier;

#[allow(clippy::too_many_arguments)]
#[tracing::instrument(skip(item, cwd, workflow, notifier, pool, worker_contexts_text), fields(task_id = item.id, trigger, provider = "codex"))]
pub(super) async fn spawn_codex_review(
    item: &mut Task,
    trigger: &str,
    db_status: Option<&str>,
    cwd: std::path::PathBuf,
    parsed_trigger: ReviewTrigger,
    worker_contexts_text: String,
    workflow: &CaptainWorkflow,
    notifier: &Notifier,
    pool: &sqlx::SqlitePool,
) -> Result<()> {
    let prompt = super::captain_review_helpers::build_review_prompt(
        item,
        trigger,
        parsed_trigger,
        worker_contexts_text,
        workflow,
        pool,
    )
    .await?;
    let mut transition_check = item.clone();
    lifecycle::apply_transition(&mut transition_check, ItemStatus::CaptainReviewing)?;

    let started = super::agent_runtime::spawn_structured_session(
        item.provider,
        pool,
        "captain-review-async",
        item.id,
        &item.project,
        "",
        &cwd,
        &prompt,
        super::captain_review::codex_verdict_output_schema(trigger),
        None,
        &workflow.agent,
    )
    .await?;

    let guard_status = review_guard_status(item, db_status);
    let prev_status = item.status;
    let saved_last_activity = item.last_activity_at.clone();
    if let Err(e) = lifecycle::apply_transition(item, ItemStatus::CaptainReviewing) {
        session_terminate::terminate_session(
            pool,
            &started.session_id,
            global_types::SessionStatus::Stopped,
            None,
        )
        .await;
        return Err(e).context("review transition failed after Codex spawn");
    }
    item.captain_review_trigger = Some(parsed_trigger);
    item.last_activity_at = Some(global_types::now_rfc3339());
    item.session_ids.review = Some(started.session_id.clone());

    let event = review_started_event(trigger, &started.session_id);
    match crate::io::queries::tasks::persist_status_transition(pool, item, &guard_status, &event)
        .await
    {
        Ok(true) => {
            notify_review_started(notifier, item, trigger).await;
            tracing::info!(module = "captain", session_id = %started.session_id, "codex review session spawned");
            Ok(())
        }
        Ok(false) => {
            rollback_review_spawn(item, prev_status, saved_last_activity);
            session_terminate::terminate_session(
                pool,
                &started.session_id,
                global_types::SessionStatus::Stopped,
                None,
            )
            .await;
            tracing::info!(
                module = "captain",
                item_id = item.id,
                "review spawn transition already applied"
            );
            Ok(())
        }
        Err(e) => {
            rollback_review_spawn(item, prev_status, saved_last_activity);
            session_terminate::terminate_session(
                pool,
                &started.session_id,
                global_types::SessionStatus::Stopped,
                None,
            )
            .await;
            Err(anyhow::anyhow!(
                "persist_status_transition failed for review spawn: {e}"
            ))
        }
    }
}
