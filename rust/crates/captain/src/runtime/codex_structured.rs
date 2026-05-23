use std::path::Path;

use anyhow::Result;
use serde_json::json;

use super::codex_app_server::{start_codex_turn, watch_codex_turn, CodexOutputMode};
use super::codex_output_schema::CodexOutputSchema;
use super::codex_stream::{append_jsonl, CodexStreamLine};

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
    let stream_path =
        global_infra::paths::codex_derived_stream_path_for_session(&started.thread_id);
    let setup_result: Result<()> = async {
        append_jsonl(
            &stream_path,
            CodexStreamLine(json!({
                "type": "system",
                "subtype": "init",
                "session_id": &started.thread_id,
                "provider": "codex",
                "cwd": cwd.display().to_string(),
            })),
        )
        .await?;
        append_jsonl(
            &stream_path,
            CodexStreamLine(json!({
                "type": "user",
                "message": {"content": [{"type": "text", "text": prompt}]},
            })),
        )
        .await?;

        crate::io::pid_registry::register(&started.thread_id, started.pid)?;
        let resumed_at = resume_thread_id.map(|_| global_types::now_rfc3339());
        let created_at = if resume_thread_id.is_some() {
            String::new()
        } else {
            global_types::now_rfc3339()
        };
        sessions_db::upsert_session(
            pool,
            &sessions_db::SessionUpsert {
                provider: global_types::TaskProvider::Codex,
                session_id: &started.thread_id,
                created_at: &created_at,
                caller,
                cwd: &cwd.display().to_string(),
                model: &started.model,
                status: global_types::SessionStatus::Running,
                cost_usd: None,
                duration_ms: None,
                resumed: resume_thread_id.is_some(),
                task_id: Some(task_id),
                scout_item_id: None,
                worker_name: if worker_name.is_empty() {
                    None
                } else {
                    Some(worker_name)
                },
                resumed_at: resumed_at.as_deref(),
                credential_id: None,
                error: None,
                api_error_status: None,
            },
        )
        .await?;

        global_claude::write_stream_meta_at(
            &global_infra::paths::codex_derived_stream_meta_path_for_session(&started.thread_id),
            &global_claude::SessionMeta {
                session_id: &started.thread_id,
                caller,
                task_id: &task_id.to_string(),
                worker_name,
                project,
                cwd: &cwd.display().to_string(),
            },
            "running",
        );
        Ok(())
    }
    .await;
    if let Err(e) = setup_result {
        super::codex_app_server::abort_started_turn(started, None, "structured setup failed").await;
        return Err(e);
    }

    let session_id = started.thread_id.clone();
    watch_codex_turn(started, pool.clone(), stream_path.clone());

    Ok(CodexStructuredSession { session_id })
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
