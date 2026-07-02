//! Provider-neutral task ask/advisor session runner.
//!
//! The bridge dispatches Claude through the existing in-memory session runtime
//! and Codex through captain's AgentRuntime text-session facade.

use std::path::Path;

use crate::AppState;

const PENDING_SESSION_ID: &str = "pending";

fn has_resumable_session_id(existing_session_id: Option<&str>) -> bool {
    match existing_session_id.map(str::trim) {
        Some(session_id) => !session_id.is_empty() && session_id != PENDING_SESSION_ID,
        None => false,
    }
}

pub(crate) fn should_resume_task_session(
    provider: api_types::TaskProvider,
    manager_has_session: bool,
    existing_session_id: Option<&str>,
) -> bool {
    match provider {
        api_types::TaskProvider::Claude => {
            manager_has_session && has_resumable_session_id(existing_session_id)
        }
        api_types::TaskProvider::Codex => has_resumable_session_id(existing_session_id),
        api_types::TaskProvider::OpenCode => false,
    }
}

pub(crate) fn should_clear_missing_manager_session(
    provider: api_types::TaskProvider,
    manager_has_session: bool,
    existing_session_id: Option<&str>,
) -> bool {
    provider == api_types::TaskProvider::Claude
        && !manager_has_session
        && has_resumable_session_id(existing_session_id)
}

pub(crate) fn should_close_orphan_manager_session(
    provider: api_types::TaskProvider,
    manager_has_session: bool,
    existing_session_id: Option<&str>,
) -> bool {
    provider == api_types::TaskProvider::Claude
        && manager_has_session
        && !has_resumable_session_id(existing_session_id)
}

pub(crate) struct TaskAgentSessionRequest<'a> {
    pub(crate) state: &'a AppState,
    pub(crate) sessions: &'a sessions::SessionsRuntime,
    pub(crate) session_key: &'a str,
    pub(crate) item: &'a captain::Task,
    pub(crate) should_resume: bool,
    pub(crate) existing_session_id: Option<&'a str>,
    pub(crate) start_prompt: String,
    pub(crate) follow_up_message: String,
    pub(crate) cwd: &'a Path,
    pub(crate) workflow: &'a settings::CaptainWorkflow,
}

pub(crate) async fn run_task_agent_session(
    request: TaskAgentSessionRequest<'_>,
) -> anyhow::Result<sessions::SessionAiResult> {
    match request.item.provider {
        api_types::TaskProvider::Claude => run_claude_task_session(request).await,
        api_types::TaskProvider::Codex => run_codex_task_session(request).await,
        api_types::TaskProvider::OpenCode => {
            anyhow::bail!("task text sessions are not enabled for OpenCode")
        }
    }
}

async fn run_claude_task_session(
    request: TaskAgentSessionRequest<'_>,
) -> anyhow::Result<sessions::SessionAiResult> {
    if request.should_resume {
        request
            .sessions
            .follow_up(sessions::SessionFollowUpRequest {
                key: request.session_key.to_string(),
                message: request.follow_up_message,
                cwd: request.cwd.to_path_buf(),
            })
            .await
    } else {
        request
            .sessions
            .start_with_item(sessions::SessionStartRequest {
                key: request.session_key.to_string(),
                prompt: request.start_prompt,
                cwd: request.cwd.to_path_buf(),
                model: Some(request.workflow.models.captain.clone()),
                idle_ttl: request.workflow.agent.task_ask_idle_ttl_s,
                call_timeout: request.workflow.agent.task_ask_timeout_s,
                task_id: Some(request.item.id),
                max_turns: None,
            })
            .await
    }
}

async fn run_codex_task_session(
    request: TaskAgentSessionRequest<'_>,
) -> anyhow::Result<sessions::SessionAiResult> {
    let resume_session_id = request
        .should_resume
        .then_some(request.existing_session_id)
        .flatten();
    let prompt = if request.should_resume {
        request.follow_up_message
    } else {
        request.start_prompt
    };
    let result = request
        .state
        .captain
        .run_task_text_session(
            request.item,
            request.session_key,
            request.cwd,
            &prompt,
            resume_session_id,
            request.workflow.agent.task_ask_timeout_s,
            &request.workflow.agent,
        )
        .await?;
    Ok(sessions::SessionAiResult {
        text: result.text,
        structured: result
            .structured
            .map(sessions::SessionStructuredOutput::from),
        session_id: result.session_id,
        cost_usd: result.cost_usd,
        duration_ms: result.duration_ms,
        duration_api_ms: result.duration_api_ms,
        num_turns: result.num_turns,
        errors: result.errors,
        envelope: result.envelope,
        stream_path: result.stream_path,
        rate_limit: result.rate_limit,
        pid: result.pid,
        credential_id: result.credential_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_resume_requires_manager_and_stored_session() {
        assert!(should_resume_task_session(
            api_types::TaskProvider::Claude,
            true,
            Some("sid")
        ));
        assert!(!should_resume_task_session(
            api_types::TaskProvider::Claude,
            false,
            Some("sid")
        ));
    }

    #[test]
    fn codex_resume_uses_stored_thread_id_without_manager_state() {
        assert!(should_resume_task_session(
            api_types::TaskProvider::Codex,
            false,
            Some("thread-id")
        ));
    }

    #[test]
    fn codex_resume_rejects_pending_placeholder() {
        assert!(!should_resume_task_session(
            api_types::TaskProvider::Codex,
            false,
            Some("pending")
        ));
        assert!(!should_resume_task_session(
            api_types::TaskProvider::Codex,
            false,
            Some("  ")
        ));
    }

    #[test]
    fn claude_resume_rejects_pending_placeholder() {
        assert!(!should_resume_task_session(
            api_types::TaskProvider::Claude,
            true,
            Some("pending")
        ));
        assert!(should_close_orphan_manager_session(
            api_types::TaskProvider::Claude,
            true,
            Some("pending")
        ));
    }

    #[test]
    fn stale_manager_cleanup_is_claude_only() {
        assert!(should_clear_missing_manager_session(
            api_types::TaskProvider::Claude,
            false,
            Some("sid")
        ));
        assert!(!should_clear_missing_manager_session(
            api_types::TaskProvider::Codex,
            false,
            Some("thread-id")
        ));
    }
}
