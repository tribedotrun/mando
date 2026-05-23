use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use futures::FutureExt;
use serde_json::{json, Value};
use tokio::time::{sleep, Duration};

use super::codex_stream::{
    append_jsonl, append_result, collect_agent_text, handle_notification, turn_status,
    CodexNotification, CodexStreamLine, CodexStreamState, CodexStructuredOutput,
};

const SESSION_STATUS_UPDATE_ATTEMPTS: u32 = 3;
const SESSION_STATUS_UPDATE_RETRY_DELAY: Duration = Duration::from_millis(250);

pub(super) fn watch_codex_turn(
    started: global_codex_app_server::StartedTurn,
    pool: sqlx::SqlitePool,
    stream_path: PathBuf,
) {
    tokio::spawn(async move {
        let thread_id = started.thread_id.clone();
        let response_timeout = started.response_timeout;
        let result = AssertUnwindSafe(run_watch_loop(started, &pool, &stream_path))
            .catch_unwind()
            .await;
        if result.is_err() {
            finalize_watcher_panic(&thread_id, response_timeout, &pool, &stream_path).await;
        }
    });
}

async fn run_watch_loop(
    mut started: global_codex_app_server::StartedTurn,
    pool: &sqlx::SqlitePool,
    stream_path: &Path,
) {
    let mut stream_state = CodexStreamState::default();
    let mut status = global_types::SessionStatus::Failed;
    let mut agent_text = String::new();
    let mut final_agent_text: Option<String> = None;
    let mut io_failure_text: Option<String> = None;
    while let Some(event) = started.events.recv().await {
        match event {
            global_codex_app_server::AppServerEvent::Notification(value) => {
                collect_agent_text(
                    CodexNotification(&value),
                    &mut agent_text,
                    &mut final_agent_text,
                );
                if handle_notification(stream_path, CodexNotification(&value), &mut stream_state)
                    .await
                {
                    status = turn_status(CodexNotification(&value));
                    break;
                }
            }
            global_codex_app_server::AppServerEvent::Fatal(message) => {
                io_failure_text = Some(message);
                break;
            }
        }
    }
    if io_failure_text.is_none() && status == global_types::SessionStatus::Failed {
        io_failure_text = Some("Codex app-server event stream closed before turn/completed".into());
    }
    finalize_turn(
        &started.thread_id,
        &started.model,
        started.expects_structured_output,
        &started.stderr_tail,
        &stream_state,
        status,
        agent_text,
        final_agent_text,
        io_failure_text,
        pool,
        stream_path,
    )
    .await;
    cleanup_after_watch(&started.thread_id, started.response_timeout).await;
}

#[allow(clippy::too_many_arguments)]
async fn finalize_turn(
    thread_id: &str,
    model: &str,
    expects_structured_output: bool,
    stderr_tail: &global_codex_app_server::StderrTail,
    stream_state: &CodexStreamState,
    mut status: global_types::SessionStatus,
    agent_text: String,
    final_agent_text: Option<String>,
    io_failure_text: Option<String>,
    pool: &sqlx::SqlitePool,
    stream_path: &Path,
) {
    let result_text = final_agent_text
        .as_deref()
        .or_else(|| (!agent_text.is_empty()).then_some(agent_text.as_str()));
    let stderr_tail_text = stderr_tail_text(stderr_tail);
    let failure_text = if status == global_types::SessionStatus::Failed {
        stream_state
            .failure_text()
            .or(io_failure_text)
            .or_else(|| (!stderr_tail_text.is_empty()).then_some(stderr_tail_text))
    } else {
        None
    };
    let result_text = result_text.or(failure_text.as_deref());
    let (structured_output, structured_error) =
        parse_structured_output(result_text, expects_structured_output);
    if status == global_types::SessionStatus::Failed
        && global_codex_app_server::shared_manager()
            .take_interrupted(thread_id)
            .await
    {
        status = global_types::SessionStatus::Stopped;
    }
    let duration_ms = stream_state.duration_ms();
    let cost_usd = stream_state.estimated_cost_usd(model);
    if let Err(e) = append_result(
        stream_path,
        status,
        stream_state,
        cost_usd,
        result_text,
        structured_output,
        structured_error.as_deref(),
    )
    .await
    {
        tracing::error!(module = "codex_app_server", thread_id, error = %e, "failed to append Codex result to stream");
        status = global_types::SessionStatus::Failed;
    }
    global_claude::update_stream_meta_status_at(
        &global_infra::paths::codex_derived_stream_meta_path_for_session(thread_id),
        thread_id,
        stream_meta_status(status),
        cost_usd,
    );
    if let Err(e) =
        update_session_status_with_retry(pool, thread_id, status, cost_usd, duration_ms).await
    {
        let marker = format!("Codex session status update failed after retries: {e}");
        tracing::error!(module = "codex_app_server", thread_id, error = %e, "failed to update Codex session status after retries");
        if let Err(append_error) = append_jsonl(
            stream_path,
            CodexStreamLine(json!({"type": "tool_result", "content": marker})),
        )
        .await
        {
            tracing::warn!(module = "codex_app_server", thread_id, error = %append_error, "failed to append Codex session-status error marker");
        }
    }
}

async fn update_session_status_with_retry(
    pool: &sqlx::SqlitePool,
    thread_id: &str,
    status: global_types::SessionStatus,
    cost_usd: Option<f64>,
    duration_ms: Option<i64>,
) -> Result<()> {
    for attempt in 1..=SESSION_STATUS_UPDATE_ATTEMPTS {
        match sessions_db::update_session_status_with_cost(
            pool,
            thread_id,
            status,
            cost_usd,
            duration_ms,
            Some(1),
        )
        .await
        {
            Ok(()) => return Ok(()),
            Err(e) if attempt == SESSION_STATUS_UPDATE_ATTEMPTS => {
                return Err(e).context("update Codex session status")
            }
            Err(e) => {
                tracing::warn!(module = "codex_app_server", thread_id, attempt, error = %e, "failed to update Codex session status; retrying");
                sleep(SESSION_STATUS_UPDATE_RETRY_DELAY).await;
            }
        }
    }
    anyhow::bail!("exhausted Codex session status update attempts")
}

async fn cleanup_after_watch(thread_id: &str, response_timeout: Duration) {
    let manager = global_codex_app_server::shared_manager();
    if let Err(e) = manager.unsubscribe(thread_id, response_timeout).await {
        tracing::debug!(module = "codex_app_server", thread_id, error = %e, "failed to unsubscribe Codex thread after turn");
    }
    manager.cleanup_thread(thread_id).await;
    if let Err(e) = crate::io::pid_registry::unregister(thread_id) {
        tracing::warn!(module = "codex_app_server", thread_id, error = %e, "failed to unregister Codex pid");
    }
    super::worker_exit::WORKER_EXIT_SIGNAL.notify_one();
}

async fn finalize_watcher_panic(
    thread_id: &str,
    response_timeout: Duration,
    pool: &sqlx::SqlitePool,
    stream_path: &Path,
) {
    let status = global_types::SessionStatus::Failed;
    let message = "Codex app-server watcher panicked before session finalization";
    tracing::error!(
        module = "codex_app_server",
        thread_id,
        "Codex watcher panicked"
    );
    let state = CodexStreamState::default();
    if let Err(e) =
        append_result(stream_path, status, &state, None, Some(message), None, None).await
    {
        tracing::error!(module = "codex_app_server", thread_id, error = %e, "failed to append Codex panic result");
    }
    global_claude::update_stream_meta_status_at(
        &global_infra::paths::codex_derived_stream_meta_path_for_session(thread_id),
        thread_id,
        stream_meta_status(status),
        None,
    );
    if let Err(e) = update_session_status_with_retry(pool, thread_id, status, None, None).await {
        tracing::error!(module = "codex_app_server", thread_id, error = %e, "failed to mark panicked Codex session failed");
    }
    cleanup_after_watch(thread_id, response_timeout).await;
}

fn stream_meta_status(status: global_types::SessionStatus) -> &'static str {
    match status {
        global_types::SessionStatus::Running => "running",
        global_types::SessionStatus::Stopped => "done",
        global_types::SessionStatus::Failed => "failed",
    }
}

fn parse_structured_output(
    result_text: Option<&str>,
    expects_structured_output: bool,
) -> (Option<CodexStructuredOutput>, Option<String>) {
    if !expects_structured_output {
        return (None, None);
    }
    let Some(text) = result_text else {
        return (None, None);
    };
    match serde_json::from_str::<Value>(text) {
        Ok(value) => (Some(CodexStructuredOutput(value)), None),
        Err(e) => {
            let preview = text.chars().take(500).collect::<String>();
            let message = format!("invalid structured JSON: {e}; raw preview: {preview}");
            tracing::warn!(module = "codex_app_server", error = %e, preview = %preview, "failed to parse Codex structured output");
            (None, Some(message))
        }
    }
}

fn stderr_tail_text(tail: &global_codex_app_server::StderrTail) -> String {
    match tail.lock() {
        Ok(lines) => lines.iter().cloned().collect::<Vec<_>>().join("\n"),
        Err(_) => String::new(),
    }
}
