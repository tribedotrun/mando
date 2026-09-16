use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;
use std::time::Instant;

use anyhow::Result;
use tokio::io::{AsyncReadExt, BufReader};

use super::codex_stream::{append_jsonl, CodexStreamLine};

const FINALIZATION_SETTLE_GRACE: Duration = Duration::from_millis(500);

#[tracing::instrument(skip_all, fields(session_id))]
pub(super) async fn write_initial_stream(
    stream_path: &Path,
    session_id: &str,
    cwd: &Path,
    prompt: &str,
) -> Result<()> {
    for line in global_opencode::initial_stream_lines(session_id, cwd, prompt) {
        append_jsonl(stream_path, CodexStreamLine(line)).await?;
    }
    Ok(())
}

#[tracing::instrument(skip_all)]
pub(super) async fn append_opencode_event(
    stream_path: &Path,
    event: &global_opencode::OpenCodeEvent,
    state: &mut global_opencode::OpenCodeStreamState,
) -> Result<()> {
    for line in global_opencode::normalize_event_lines(event, state) {
        append_jsonl(stream_path, CodexStreamLine(line)).await?;
    }
    Ok(())
}

pub(super) fn watch_opencode_worker(
    started_run: global_opencode::StartedOpenCodeRun,
    pool: sqlx::SqlitePool,
    session_id: String,
    stream_path: PathBuf,
    mut state: global_opencode::OpenCodeStreamState,
    pid_identity: crate::io::pid_registry::PidEntry,
) {
    tokio::spawn(async move {
        let start = Instant::now();
        let mut child = started_run.child;
        let mut lines = started_run.stdout_lines;
        let stderr_task = tokio::spawn(read_stderr(started_run.stderr));
        let mut stdout_error: Option<String> = None;
        loop {
            match lines.next_line().await {
                Ok(Some(line)) => match global_opencode::OpenCodeEvent::parse(&line) {
                    Ok(event) => {
                        if let Err(e) =
                            append_opencode_event(&stream_path, &event, &mut state).await
                        {
                            stdout_error = Some(e.to_string());
                            break;
                        }
                    }
                    Err(e) => {
                        stdout_error = Some(format!("failed to parse OpenCode JSON event: {e}"));
                        break;
                    }
                },
                Ok(None) => break,
                Err(e) => {
                    stdout_error = Some(format!("failed to read OpenCode stdout: {e}"));
                    break;
                }
            }
        }
        if stdout_error.is_some() {
            if let Err(e) = global_opencode::terminate_process(pid_identity.pid).await {
                tracing::warn!(
                    module = "opencode_worker_spawn",
                    session_id,
                    pid = %pid_identity.pid,
                    error = %e,
                    "failed to terminate OpenCode process after stdout error"
                );
            }
        }
        let status = child.wait().await;
        let stderr_text = stderr_task.await.unwrap_or_default();
        finalize_opencode_worker(FinalizeOpenCodeWorker {
            pool: &pool,
            session_id: &session_id,
            stream_path: &stream_path,
            state,
            status,
            stdout_error,
            stderr_text: &stderr_text,
            duration_ms: start.elapsed().as_millis().try_into().unwrap_or(i64::MAX),
            pid_identity: &pid_identity,
        })
        .await;
        if let Err(e) =
            crate::io::pid_registry::unregister_entry_if_current(&session_id, &pid_identity)
        {
            tracing::warn!(module = "opencode_worker_spawn", session_id, error = %e, "failed to unregister OpenCode pid");
        }
        super::worker_exit::WORKER_EXIT_SIGNAL.notify_one();
    });
}

async fn read_stderr(stderr: Option<tokio::process::ChildStderr>) -> String {
    let Some(stderr) = stderr else {
        return String::new();
    };
    let mut reader = BufReader::new(stderr);
    let mut text = String::new();
    match reader.read_to_string(&mut text).await {
        Ok(_) => text,
        Err(e) => format!("failed to read OpenCode stderr: {e}"),
    }
}

struct FinalizeOpenCodeWorker<'a> {
    pool: &'a sqlx::SqlitePool,
    session_id: &'a str,
    stream_path: &'a Path,
    state: global_opencode::OpenCodeStreamState,
    status: std::io::Result<std::process::ExitStatus>,
    stdout_error: Option<String>,
    stderr_text: &'a str,
    duration_ms: i64,
    pid_identity: &'a crate::io::pid_registry::PidEntry,
}

async fn finalize_opencode_worker(request: FinalizeOpenCodeWorker<'_>) {
    let process_unsuccessful = !request
        .status
        .as_ref()
        .is_ok_and(std::process::ExitStatus::success);
    let Some(decision) = finalization_decision(
        request.pool,
        request.session_id,
        request.pid_identity,
        process_unsuccessful,
    )
    .await
    else {
        return;
    };
    let completion = global_opencode::result_stream_line(
        request.state,
        request.status,
        request.stdout_error,
        request.stderr_text,
        request.duration_ms,
        decision.expected_termination,
    );
    if let Err(e) = append_jsonl(request.stream_path, CodexStreamLine(completion.result_line)).await
    {
        tracing::error!(module = "opencode_worker_spawn", session_id = request.session_id, error = %e, "failed to append OpenCode result");
    }
    global_claude::update_stream_meta_status_at(
        &global_infra::paths::opencode_stream_meta_path_for_session(request.session_id),
        request.session_id,
        stream_meta_status(completion.final_status),
        completion.cost_usd,
    );
    if let Err(e) = sessions_db::update_session_status_with_cost(
        request.pool,
        request.session_id,
        completion.final_status,
        completion.cost_usd,
        Some(request.duration_ms),
        Some(1),
    )
    .await
    {
        tracing::error!(module = "opencode_worker_spawn", session_id = request.session_id, error = %e, "failed to update OpenCode session status");
    }
}

struct FinalizationDecision {
    expected_termination: bool,
}

async fn finalization_decision(
    pool: &sqlx::SqlitePool,
    session_id: &str,
    pid_identity: &crate::io::pid_registry::PidEntry,
    process_unsuccessful: bool,
) -> Option<FinalizationDecision> {
    finalization_decision_with_grace(
        pool,
        session_id,
        pid_identity,
        process_unsuccessful,
        FINALIZATION_SETTLE_GRACE,
    )
    .await
}

async fn finalization_decision_with_grace(
    pool: &sqlx::SqlitePool,
    session_id: &str,
    pid_identity: &crate::io::pid_registry::PidEntry,
    process_unsuccessful: bool,
    settle_grace: Duration,
) -> Option<FinalizationDecision> {
    if skip_replaced_process(session_id, pid_identity) {
        return None;
    }

    if process_unsuccessful {
        tokio::time::sleep(settle_grace).await;
        if skip_replaced_process(session_id, pid_identity) {
            return None;
        }
    }

    Some(FinalizationDecision {
        expected_termination: session_was_stopped(pool, session_id).await,
    })
}

fn skip_replaced_process(
    session_id: &str,
    pid_identity: &crate::io::pid_registry::PidEntry,
) -> bool {
    let Some(current_identity) = crate::io::pid_registry::get_entry(session_id) else {
        return false;
    };
    if current_identity == *pid_identity {
        return false;
    }
    tracing::info!(
        module = "opencode_worker_spawn",
        session_id,
        old_pid = %pid_identity.pid,
        current_pid = %current_identity.pid,
        old_started_at = %pid_identity.started_at,
        current_started_at = %current_identity.started_at,
        "skipping stale OpenCode finalizer for replaced worker process"
    );
    true
}

async fn session_was_stopped(pool: &sqlx::SqlitePool, session_id: &str) -> bool {
    match sessions_db::session_by_id(pool, session_id).await {
        Ok(Some(row)) => row.status == global_types::SessionStatus::Stopped.as_str(),
        Ok(None) => false,
        Err(e) => {
            tracing::warn!(module = "opencode_worker_spawn", session_id, error = %e, "failed to read OpenCode session status before finalization");
            false
        }
    }
}

fn stream_meta_status(status: global_types::SessionStatus) -> &'static str {
    match status {
        global_types::SessionStatus::Running => "running",
        global_types::SessionStatus::Stopped => "done",
        global_types::SessionStatus::Failed => "failed",
    }
}
