//! Provider-neutral nudge/resume delivery for active workers.

use anyhow::Result;
use settings::CaptainWorkflow;

use crate::{Pid, Task};

pub(crate) struct AgentNudgeDelivery {
    pub(crate) pid: Pid,
    pub(crate) stream_size_before: u64,
}

pub(crate) enum AgentNudgeOutcome {
    Delivered(AgentNudgeDelivery),
    BrokenSession { alert: String },
}

#[allow(clippy::too_many_arguments)]
#[tracing::instrument(skip_all, fields(provider = %item.provider.as_str(), task_id = item.id, session_id))]
pub(crate) async fn nudge_worker(
    pool: &sqlx::SqlitePool,
    item: &Task,
    worker_name: &str,
    cwd: &std::path::Path,
    prompt: &str,
    session_id: &str,
    model: &str,
    workflow: &CaptainWorkflow,
    current_pid: Pid,
) -> Result<AgentNudgeOutcome> {
    let fallback_provider = super::agent_runtime::implementation_provider(item, workflow);
    let provider =
        super::agent_runtime::persisted_session_provider(pool, session_id, fallback_provider).await;
    let stream_path = super::agent_session_result::stream_path(provider, session_id);
    if let Some(reason) =
        super::agent_runtime::worker_resume_replacement_reason(provider, &stream_path, workflow)
    {
        if current_pid.as_u32() > 0 {
            if let Err(e) = crate::io::pid_registry::register(session_id, current_pid) {
                tracing::warn!(
                    module = "agent_runtime",
                    worker = %worker_name,
                    session_id,
                    pid = %current_pid,
                    error = %e,
                    "failed to refresh stale worker pid before broken-session cleanup"
                );
            }
        }
        super::agent_runtime::terminate_worker_process(provider, session_id).await?;
        return Ok(AgentNudgeOutcome::BrokenSession {
            alert: format!(
                "Broken session symptom ({reason}) for {worker_name} — captain review triggered"
            ),
        });
    }

    let stream_size_before = super::agent_session_result::stream_file_size(provider, session_id);
    if super::agent_runtime::steer(provider, session_id, prompt.to_string()).await? {
        return Ok(AgentNudgeOutcome::Delivered(AgentNudgeDelivery {
            pid: current_pid,
            stream_size_before,
        }));
    }
    let resume = super::agent_runtime::resume_worker(
        pool,
        item,
        worker_name,
        cwd,
        prompt,
        session_id,
        model,
        workflow,
    )
    .await?;
    Ok(AgentNudgeOutcome::Delivered(AgentNudgeDelivery {
        pid: resume.pid,
        stream_size_before,
    }))
}
