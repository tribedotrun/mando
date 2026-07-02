use std::path::Path;

use anyhow::Result;

use crate::Task;

pub(super) struct ClaudeWorkerResume {
    pub(super) pid: crate::Pid,
}

/// Resume an existing Claude Code worker session.
///
/// This is the Claude adapter half of the provider-neutral AgentRuntime
/// boundary. Lifecycle code should ask AgentRuntime to resume a worker instead
/// of reaching into Claude-specific process-manager helpers directly.
#[tracing::instrument(skip(pool, item, prompt), fields(task_id = item.id, session_id, provider = "claude"))]
pub(super) async fn resume_worker(
    pool: &sqlx::SqlitePool,
    item: &Task,
    worker_name: &str,
    cwd: &Path,
    prompt: &str,
    session_id: &str,
    model: &str,
) -> Result<ClaudeWorkerResume> {
    if let Some(pid) = crate::io::pid_lookup::resolve_pid(session_id, worker_name) {
        if pid.as_u32() > 0 {
            if let Err(e) = global_claude::kill_process(pid).await {
                tracing::warn!(
                    module = "agent_runtime",
                    worker = %worker_name,
                    pid = %pid,
                    error = %e,
                    "failed to kill old Claude process before resume"
                );
            }
        }
    }

    let (mut env, credential_id) =
        super::spawner::credential_env_for_session(pool, session_id).await;
    env.insert("MANDO_TASK_ID".to_string(), item.id.to_string());
    let (pid, _stream_path) =
        crate::io::process_manager::resume_worker_process(prompt, cwd, model, session_id, &env)
            .await?;
    crate::io::pid_registry::register(session_id, pid)?;

    crate::io::headless_cc::log_running_session(
        pool,
        session_id,
        cwd,
        model,
        "worker",
        worker_name,
        Some(item.id),
        true,
        credential_id,
    )
    .await?;

    Ok(ClaudeWorkerResume { pid })
}

#[tracing::instrument(skip_all, fields(provider = "claude", session_id))]
pub(super) async fn terminate_worker_process(session_id: &str) -> Result<()> {
    if let Some(pid) = crate::io::pid_registry::get_verified_pid(session_id) {
        if pid.as_u32() > 0 && global_claude::is_process_alive(pid) {
            global_claude::kill_process(pid).await?;
        }
    }
    Ok(())
}

pub(super) fn broken_resume_reason(
    stream_path: &Path,
    workflow: &settings::CaptainWorkflow,
) -> Option<String> {
    if global_claude::stream_has_broken_session(stream_path) {
        return Some("no init event in stream".to_string());
    }
    let symptoms = global_claude::StreamSymptomMatcher::new(workflow.stream_symptoms.clone());
    global_claude::stream_broken_session_symptom(stream_path, &symptoms)
        .map(|m| format!("{} ({})", m.reason, m.origin.tag()))
}
