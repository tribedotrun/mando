use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AgentLivenessStatus {
    Active,
    Inactive,
    Completed,
    Interrupted,
    Failed,
}

impl AgentLivenessStatus {
    pub(crate) fn is_active(self) -> bool {
        matches!(self, Self::Active)
    }

    pub(crate) fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Interrupted | Self::Failed)
    }
}

#[tracing::instrument(fields(provider = %provider.as_str(), session_id, pid = %pid))]
pub(crate) async fn session_liveness(
    provider: global_types::TaskProvider,
    session_id: &str,
    pid: crate::Pid,
    stream_path: &Path,
) -> AgentLivenessStatus {
    let provider_active = super::agent_runtime::Adapter::new(provider)
        .is_active(session_id, pid)
        .await;
    if provider_active {
        return AgentLivenessStatus::Active;
    }
    if let Some(status) = stream_terminal_status(stream_path) {
        return status;
    }
    AgentLivenessStatus::Inactive
}

#[tracing::instrument(fields(provider = %provider.as_str(), session_id, pid = %pid))]
pub(crate) async fn is_session_active(
    provider: global_types::TaskProvider,
    session_id: &str,
    pid: crate::Pid,
) -> bool {
    let stream_path = super::agent_session_result::stream_path(provider, session_id);
    session_liveness(provider, session_id, pid, &stream_path)
        .await
        .is_active()
}

fn stream_terminal_status(stream_path: &Path) -> Option<AgentLivenessStatus> {
    let result = global_claude::get_stream_result(stream_path)?;
    match global_claude::result_outcome(&result) {
        global_claude::ResultOutcome::Success => Some(AgentLivenessStatus::Completed),
        global_claude::ResultOutcome::Interrupted => Some(AgentLivenessStatus::Interrupted),
        _ => Some(AgentLivenessStatus::Failed),
    }
}
