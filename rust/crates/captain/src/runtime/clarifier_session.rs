//! Provider-neutral initial and follow-up clarifier phase runner.

use anyhow::{Context, Result};
use global_types::TaskOwnerProvider;
use settings::{CaptainWorkflow, Config};

use crate::service::lifecycle;
use crate::{ItemStatus, Task};

use super::clarifier::{
    build_clarifier_prompt, build_clarifier_schema, parse_clarifier_response,
    resolve_clarifier_cwd, ClarifierResult,
};

#[tracing::instrument(skip(item, config, workflow, pool), fields(task_id = item.id, provider = %item.provider.as_str()))]
pub(super) async fn spawn_initial(
    item: &mut Task,
    config: &Config,
    workflow: &CaptainWorkflow,
    pool: &sqlx::SqlitePool,
) -> Result<Option<String>> {
    let owner = super::agent_runtime::Adapter::for_task(item)?.task_owner()?;
    match owner {
        TaskOwnerProvider::Claude => Ok(None),
        TaskOwnerProvider::Codex => spawn_codex_initial(item, config, workflow, pool)
            .await
            .map(Some),
    }
}

#[tracing::instrument(skip_all, fields(provider = %item.provider.as_str(), task_id = item.id))]
pub(super) async fn answer_followup(
    item: &Task,
    prompt: &str,
    cwd: &std::path::Path,
    workflow: &CaptainWorkflow,
    prior_resume_sid: Option<&str>,
    pool: &sqlx::SqlitePool,
) -> Result<ClarifierResult> {
    let owner = super::agent_runtime::Adapter::for_task(item)?.task_owner()?;
    match owner {
        TaskOwnerProvider::Claude => {
            super::agent_runtime::answer_claude_clarifier(
                item,
                prompt,
                cwd,
                workflow,
                prior_resume_sid,
                pool,
            )
            .await
        }
        TaskOwnerProvider::Codex => {
            answer_codex(item, prompt, cwd, workflow, prior_resume_sid, pool).await
        }
    }
}

async fn spawn_codex_initial(
    item: &mut Task,
    config: &Config,
    workflow: &CaptainWorkflow,
    pool: &sqlx::SqlitePool,
) -> Result<String> {
    let clarifier_cwd = match resolve_clarifier_cwd(item, config) {
        Ok(cwd) => cwd,
        Err(e) => {
            tracing::error!(module = "captain", id = item.id, error = %e, "cannot resolve Codex clarifier cwd");
            lifecycle::restore_status(item, ItemStatus::New);
            return Err(e).context("cannot resolve Codex clarifier cwd");
        }
    };
    let prompt = match build_clarifier_prompt(item, None, workflow) {
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
        build_clarifier_schema(workflow),
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
        "clarifier_session: emit Codex clarify started"
    );
    Ok(started.session_id)
}

async fn answer_codex(
    item: &Task,
    prompt: &str,
    cwd: &std::path::Path,
    workflow: &CaptainWorkflow,
    prior_resume_sid: Option<&str>,
    pool: &sqlx::SqlitePool,
) -> Result<ClarifierResult> {
    let started = super::agent_runtime::spawn_structured_session(
        item.provider,
        pool,
        "clarifier",
        item.id,
        &item.project,
        "",
        cwd,
        prompt,
        super::clarifier_cc_failure::build_interactive_clarifier_schema(workflow),
        prior_resume_sid,
        &workflow.agent,
    )
    .await?;
    let deadline = tokio::time::Instant::now() + workflow.agent.clarifier_timeout_s;
    loop {
        match super::agent_runtime::poll_structured_session_output(
            item.provider,
            &started.session_id,
        ) {
            super::agent_runtime::AgentSessionPoll::Completed(output) => {
                let text = super::agent_runtime::session_output_text(output);
                let mut parsed = parse_clarifier_response(&text, &item.title);
                parsed.session_id = Some(started.session_id);
                return Ok(parsed);
            }
            super::agent_runtime::AgentSessionPoll::Failed(error)
            | super::agent_runtime::AgentSessionPoll::UnusableOutput(error) => {
                anyhow::bail!(error);
            }
            super::agent_runtime::AgentSessionPoll::Pending => {}
        }
        if tokio::time::Instant::now() >= deadline {
            crate::io::session_terminate::terminate_session(
                pool,
                &started.session_id,
                global_types::SessionStatus::Failed,
                None,
            )
            .await;
            anyhow::bail!("Codex clarifier follow-up timed out");
        }
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    }
}
