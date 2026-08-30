use std::path::Path;

use anyhow::Result;

use super::codex_app_server::{start_codex_turn, CodexOutputMode};
use super::codex_output_schema::CodexOutputSchema;
use super::codex_session::{begin_codex_session, CodexSessionSpec};

pub(super) struct CodexStructuredSession {
    pub(crate) session_id: String,
}

#[allow(clippy::too_many_arguments)]
#[tracing::instrument(skip_all)]
pub(super) async fn spawn_structured_session(
    pool: &sqlx::SqlitePool,
    caller: &str,
    task_id: i64,
    project: &str,
    worker_name: &str,
    cwd: &Path,
    prompt: &str,
    output_schema: CodexOutputSchema,
    resume_thread_id: Option<&str>,
    agent_config: &settings::AgentConfig,
) -> Result<CodexStructuredSession> {
    let prompt = codex_structured_prompt(prompt);
    let started = start_codex_turn(
        cwd,
        &prompt,
        resume_thread_id,
        CodexOutputMode::Structured(output_schema),
        agent_config,
    )
    .await?;
    let session = begin_codex_session(
        pool,
        started,
        CodexSessionSpec {
            caller,
            task_id,
            project,
            worker_name: (!worker_name.is_empty()).then_some(worker_name),
            cwd,
            prompt: &prompt,
            resumed: resume_thread_id.is_some(),
            alias: None,
            abort_reason: "structured setup failed",
        },
    )
    .await?;

    Ok(CodexStructuredSession {
        session_id: session.session_id,
    })
}

fn codex_structured_prompt(prompt: &str) -> String {
    let claude_output_instruction =
        "Use the StructuredOutput tool. The JSON schema is enforced automatically.";
    let codex_output_instruction =
        "Final response: return only one valid JSON object matching the schema. Do not wrap it in markdown, code fences, or explanatory text.";
    let prompt = if prompt.contains(claude_output_instruction) {
        prompt.replace(claude_output_instruction, codex_output_instruction)
    } else {
        format!("{prompt}\n\n{codex_output_instruction}")
    };
    prompt.trim_end().to_string() + "\n"
}
