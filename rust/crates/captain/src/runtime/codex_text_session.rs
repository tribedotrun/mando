use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use serde_json::json;
use tokio::time::sleep;

use super::codex_app_server::{start_codex_turn, watch_codex_turn, CodexOutputMode};
use super::codex_stream::{append_jsonl, CodexStreamLine};
use crate::Task;

const POLL_INTERVAL: Duration = Duration::from_millis(250);
const INTERRUPT_SETTLE: Duration = Duration::from_millis(500);

#[allow(clippy::too_many_arguments)]
#[tracing::instrument(skip_all, fields(provider = "codex", task_id = item.id, caller, resume = resume_thread_id.is_some()))]
pub(super) async fn run_text_session(
    pool: &sqlx::SqlitePool,
    item: &Task,
    caller: &str,
    cwd: &Path,
    prompt: &str,
    resume_thread_id: Option<&str>,
    call_timeout: Duration,
    agent_config: &settings::AgentConfig,
) -> Result<super::agent_text_session::AgentTextSessionResult> {
    let started = start_codex_turn(
        cwd,
        prompt,
        resume_thread_id,
        CodexOutputMode::Text,
        agent_config,
    )
    .await?;
    let stream_path =
        global_infra::paths::codex_derived_stream_path_for_session(&started.thread_id);
    let session_id = started.thread_id.clone();
    let pid = started.pid;

    let setup_result: Result<()> = async {
        append_jsonl(
            &stream_path,
            CodexStreamLine(json!({
                "type": "system",
                "subtype": "init",
                "session_id": &session_id,
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

        crate::io::pid_registry::register(&session_id, pid)?;
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
                session_id: &session_id,
                created_at: &created_at,
                caller,
                cwd: &cwd.display().to_string(),
                model: &started.model,
                status: global_types::SessionStatus::Running,
                cost_usd: None,
                duration_ms: None,
                resumed: resume_thread_id.is_some(),
                task_id: Some(item.id),
                scout_item_id: None,
                worker_name: None,
                resumed_at: resumed_at.as_deref(),
                credential_id: None,
                error: None,
                api_error_status: None,
            },
        )
        .await?;
        global_claude::write_stream_meta_at(
            &global_infra::paths::codex_derived_stream_meta_path_for_session(&session_id),
            &global_claude::SessionMeta {
                session_id: &session_id,
                caller,
                task_id: &item.id.to_string(),
                worker_name: "",
                project: &item.project,
                cwd: &cwd.display().to_string(),
            },
            "running",
        );
        Ok(())
    }
    .await;
    if let Err(e) = setup_result {
        super::codex_app_server::abort_started_turn(started, None, "text session setup failed")
            .await;
        return Err(e);
    }

    watch_codex_turn(started, pool.clone(), stream_path.clone());

    wait_for_text_result(&session_id, &stream_path, pid, call_timeout).await
}

async fn wait_for_text_result(
    session_id: &str,
    stream_path: &Path,
    pid: global_types::Pid,
    call_timeout: Duration,
) -> Result<global_claude::CcResult<serde_json::Value>> {
    let started_at = Instant::now();
    loop {
        if let Some(result) = global_claude::get_stream_result(stream_path) {
            if result.get("is_error").and_then(|v| v.as_bool()) == Some(true) {
                let message = result
                    .get("error")
                    .and_then(|v| v.as_str())
                    .or_else(|| result.get("result").and_then(|v| v.as_str()))
                    .unwrap_or("Codex text session failed");
                anyhow::bail!("{message}");
            }
            let text = result
                .get("result")
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .filter(|s| !s.is_empty())
                .or_else(|| global_claude::get_last_assistant_text(stream_path))
                .context("Codex text session completed without assistant text")?;
            let cost = global_claude::session_cost_or_estimate(stream_path);
            return Ok(global_claude::CcResult {
                text,
                structured: None,
                session_id: session_id.to_string(),
                cost_usd: cost.total_cost_usd,
                duration_ms: result.get("duration_ms").and_then(|v| v.as_u64()),
                duration_api_ms: result.get("duration_api_ms").and_then(|v| v.as_u64()),
                num_turns: result
                    .get("num_turns")
                    .and_then(|v| v.as_u64())
                    .and_then(|v| u32::try_from(v).ok()),
                errors: Vec::new(),
                envelope: global_claude::CcEnvelope(result),
                stream_path: stream_path.to_path_buf(),
                rate_limit: None,
                pid,
                credential_id: None,
            });
        }

        if started_at.elapsed() >= call_timeout {
            match super::codex_app_server::interrupt(session_id).await {
                Ok(true) => sleep(INTERRUPT_SETTLE).await,
                Ok(false) => {}
                Err(e) => {
                    tracing::warn!(module = "codex_text_session", session_id, error = %e, "failed to interrupt timed-out Codex text session")
                }
            }
            anyhow::bail!(
                "Codex text session timed out after {}s",
                call_timeout.as_secs()
            );
        }
        sleep(POLL_INTERVAL).await;
    }
}
