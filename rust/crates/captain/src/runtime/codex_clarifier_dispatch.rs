use anyhow::{Context, Result};
use settings::{CaptainWorkflow, Config};

use crate::service::lifecycle;
use crate::{ItemStatus, Task};

use super::clarifier;

#[tracing::instrument(skip(item, config, workflow, pool), fields(task_id = item.id, provider = "codex"))]
pub(super) async fn spawn_codex_clarifier(
    item: &mut Task,
    config: &Config,
    workflow: &CaptainWorkflow,
    pool: &sqlx::SqlitePool,
) -> Result<String> {
    let clarifier_cwd = match clarifier::resolve_clarifier_cwd(item, config) {
        Ok(cwd) => cwd,
        Err(e) => {
            tracing::error!(module = "captain", id = item.id, error = %e, "cannot resolve Codex clarifier cwd");
            lifecycle::restore_status(item, ItemStatus::New);
            return Err(e).context("cannot resolve Codex clarifier cwd");
        }
    };
    let prompt = match clarifier::build_clarifier_prompt(item, None, workflow) {
        Ok(prompt) => prompt,
        Err(e) => {
            tracing::error!(module = "captain", id = item.id, error = %e, "cannot render Codex clarifier prompt");
            lifecycle::restore_status(item, ItemStatus::New);
            return Err(e).context("cannot render Codex clarifier prompt");
        }
    };
    let started = match super::agent_runtime::spawn_structured_session(
        item.provider,
        pool,
        "clarifier",
        item.id,
        &item.project,
        "",
        &clarifier_cwd,
        &prompt,
        clarifier::build_clarifier_schema(workflow),
        None,
        &workflow.agent,
    )
    .await
    {
        Ok(started) => started,
        Err(e) => {
            tracing::warn!(module = "captain", id = item.id, error = %e, "failed to spawn Codex clarifier");
            lifecycle::restore_status(item, ItemStatus::New);
            return Err(e).context("failed to spawn Codex clarifier");
        }
    };
    item.session_ids.clarifier = Some(started.session_id.clone());
    if let Err(e) = crate::io::queries::tasks::persist_clarify_start(pool, item).await {
        tracing::error!(module = "captain", id = item.id, error = %e, "failed to persist Codex clarify start");
        crate::io::session_terminate::terminate_session(
            pool,
            &started.session_id,
            global_types::SessionStatus::Stopped,
            None,
        )
        .await;
        lifecycle::restore_status(item, ItemStatus::New);
        item.session_ids.clarifier = None;
        return Err(e).context("failed to persist Codex clarify start");
    }
    global_infra::best_effort!(
        super::timeline_emit::emit_for_task(
            item,
            "Clarification starting",
            crate::TimelineEventPayload::ClarifyStarted {
                session_id: started.session_id.clone(),
            },
            pool,
        )
        .await,
        "dispatch_clarify: emit Codex clarify started"
    );
    Ok(started.session_id)
}
