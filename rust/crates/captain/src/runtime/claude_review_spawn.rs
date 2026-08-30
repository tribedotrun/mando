//! Claude adapter for captain review sessions.

use anyhow::Result;
use settings::CaptainWorkflow;

use crate::service::lifecycle;
use crate::{ItemStatus, Task};

use super::captain_review::codex_verdict_output_schema;
use super::captain_review_helpers::{
    build_review_prompt, notify_review_started, review_guard_status, review_started_event,
    rollback_review_spawn,
};
use super::claude_detached_session::DetachedClaudeSession;
use super::notify::Notifier;

#[allow(clippy::too_many_arguments)]
#[tracing::instrument(skip_all, fields(provider = "claude", task_id = item.id, trigger))]
pub(super) async fn spawn_claude_review(
    item: &mut Task,
    trigger: &str,
    db_status: Option<&str>,
    cwd: std::path::PathBuf,
    parsed_trigger: crate::ReviewTrigger,
    worker_contexts_text: String,
    workflow: &CaptainWorkflow,
    notifier: &Notifier,
    pool: &sqlx::SqlitePool,
) -> Result<()> {
    // Render BEFORE any state mutation. A template error here must leave the
    // task exactly where it was; rendering after the `CaptainReviewing`
    // transition strands the task in a review state with no session and no
    // way out. Mirrors the Codex adapter, which has always rendered first.
    let prompt = build_review_prompt(
        item,
        trigger,
        parsed_trigger,
        worker_contexts_text,
        workflow,
        pool,
    )
    .await?;

    // All fallible operations succeeded, now commit state changes.
    let guard_status = review_guard_status(item, db_status);
    let prev_status = item.status;
    let saved_last_activity = item.last_activity_at.clone();
    lifecycle::apply_transition(item, ItemStatus::CaptainReviewing)?;
    item.captain_review_trigger = Some(parsed_trigger);
    item.last_activity_at = Some(global_types::now_rfc3339());
    let session_id = global_infra::uuid::Uuid::v4().to_string();
    item.session_ids.review = Some(session_id.clone());

    let event = review_started_event(trigger, &session_id);
    match crate::io::queries::tasks::persist_status_transition(pool, item, &guard_status, &event)
        .await
    {
        Ok(true) => notify_review_started(notifier, item, trigger).await,
        Ok(false) => {
            rollback_review_spawn(item, prev_status, saved_last_activity);
            tracing::info!(
                module = "captain",
                item_id = item.id,
                "review spawn transition already applied"
            );
            return Ok(());
        }
        Err(e) => {
            rollback_review_spawn(item, prev_status, saved_last_activity);
            return Err(anyhow::anyhow!(
                "persist_status_transition failed for review spawn: {e}"
            ));
        }
    }

    // Pick a credential for this review session so it goes through
    // multi-credential load balancing when credentials are configured.
    let credential = super::tick_spawn::pick_credential(pool).await;

    super::claude_detached_session::spawn_detached_claude_session(DetachedClaudeSession {
        caller: "captain-review-async",
        phase: "captain review",
        session_id,
        task_id: item.id,
        cwd,
        prompt,
        model: workflow.models.captain.clone(),
        timeout: workflow.agent.captain_review_timeout_s,
        cc_max_retries: workflow.agent.cc_max_retries,
        effort: workflow.agent.cc_effort,
        allowed_tools: vec!["Read".into(), "Bash".into()],
        disallowed_tools: vec!["Agent".into()],
        json_schema: codex_verdict_output_schema(trigger).0,
        slot: crate::SessionSlot::Review,
        credential,
        notifier: notifier.fork(),
        pool: pool.clone(),
    })
    .await;

    Ok(())
}
