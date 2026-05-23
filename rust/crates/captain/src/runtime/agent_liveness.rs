use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AgentLivenessStatus {
    Active,
    Inactive,
    Completed,
    Failed,
}

impl AgentLivenessStatus {
    pub(crate) fn is_active(self) -> bool {
        matches!(self, Self::Active)
    }

    pub(crate) fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed)
    }
}

#[tracing::instrument(fields(provider = %provider.as_str(), session_id, pid = %pid))]
pub(crate) async fn session_liveness(
    provider: global_types::TaskProvider,
    session_id: &str,
    pid: crate::Pid,
    stream_path: &Path,
) -> AgentLivenessStatus {
    let provider_active = match provider {
        global_types::TaskProvider::Codex => {
            super::codex_app_server::is_turn_active(session_id).await
        }
        global_types::TaskProvider::Claude => {
            pid.as_u32() > 0 && global_claude::is_process_alive(pid)
        }
    };
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
    if global_claude::is_clean_result(&result) {
        Some(AgentLivenessStatus::Completed)
    } else {
        Some(AgentLivenessStatus::Failed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_stream(content: &str) -> tempfile::NamedTempFile {
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), content).unwrap();
        file
    }

    #[tokio::test]
    async fn codex_inactive_when_only_shared_process_pid_is_alive() {
        let file = write_stream(r#"{"type":"system","subtype":"init"}"#);
        let status = session_liveness(
            global_types::TaskProvider::Codex,
            "codex-inactive",
            crate::Pid::new(std::process::id()),
            file.path(),
        )
        .await;

        assert_eq!(status, AgentLivenessStatus::Inactive);
    }

    #[tokio::test]
    async fn claude_uses_session_process_liveness() {
        let file = write_stream(r#"{"type":"system","subtype":"init"}"#);
        let status = session_liveness(
            global_types::TaskProvider::Claude,
            "claude-active",
            crate::Pid::new(std::process::id()),
            file.path(),
        )
        .await;

        assert_eq!(status, AgentLivenessStatus::Active);
    }

    #[tokio::test]
    async fn active_claude_process_wins_over_previous_terminal_result() {
        let file = write_stream(concat!(
            r#"{"type":"system","subtype":"init"}"#,
            "\n",
            r#"{"type":"result","subtype":"success","is_error":false}"#,
            "\n"
        ));
        let status = session_liveness(
            global_types::TaskProvider::Claude,
            "claude-resumed-active",
            crate::Pid::new(std::process::id()),
            file.path(),
        )
        .await;

        assert_eq!(status, AgentLivenessStatus::Active);
    }

    #[tokio::test]
    async fn terminal_stream_result_wins_when_provider_is_inactive() {
        let file = write_stream(concat!(
            r#"{"type":"system","subtype":"init"}"#,
            "\n",
            r#"{"type":"result","subtype":"success","is_error":false}"#,
            "\n"
        ));
        let status = session_liveness(
            global_types::TaskProvider::Codex,
            "codex-complete",
            crate::Pid::new(std::process::id()),
            file.path(),
        )
        .await;

        assert_eq!(status, AgentLivenessStatus::Completed);
    }
}
