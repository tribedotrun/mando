//! Codex adapter for follow-up clarifier turns.

use anyhow::Result;
use settings::CaptainWorkflow;

use crate::io::session_terminate;
use crate::Task;

use super::clarifier::{parse_clarifier_response, ClarifierResult};

#[tracing::instrument(skip_all, fields(provider = "codex", task_id = item.id))]
pub(super) async fn answer_and_reclarify_codex(
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
            session_terminate::terminate_session(
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
