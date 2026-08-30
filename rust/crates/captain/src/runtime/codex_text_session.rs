use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use tokio::time::sleep;

use super::codex_app_server::{start_codex_turn, CodexOutputMode};
use super::codex_session::{begin_codex_session, CodexSessionSpec};
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
    let session = begin_codex_session(
        pool,
        started,
        CodexSessionSpec {
            caller,
            task_id: item.id,
            project: &item.project,
            worker_name: None,
            cwd,
            prompt,
            resumed: resume_thread_id.is_some(),
            alias: None,
            abort_reason: "text session setup failed",
        },
    )
    .await?;

    wait_for_text_result(
        &session.session_id,
        &session.stream_path,
        session.pid,
        call_timeout,
    )
    .await
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
