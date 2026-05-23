use anyhow::Result;

use crate::Task;

pub(crate) type AgentTextSessionResult = global_claude::CcResult<serde_json::Value>;

#[allow(clippy::too_many_arguments)]
#[tracing::instrument(skip(pool, item, prompt), fields(provider = %item.provider.as_str(), task_id = item.id, caller, resume = resume_session_id.is_some()))]
pub(crate) async fn run_text_session(
    pool: &sqlx::SqlitePool,
    item: &Task,
    caller: &str,
    cwd: &std::path::Path,
    prompt: &str,
    resume_session_id: Option<&str>,
    call_timeout: std::time::Duration,
    agent_config: &settings::AgentConfig,
) -> Result<AgentTextSessionResult> {
    match item.provider {
        api_types::TaskProvider::Codex => {
            super::codex_text_session::run_text_session(
                pool,
                item,
                caller,
                cwd,
                prompt,
                resume_session_id,
                call_timeout,
                agent_config,
            )
            .await
        }
        api_types::TaskProvider::Claude => {
            anyhow::bail!("direct Captain text sessions support Codex only; Claude is handled by the provider session bridge")
        }
    }
}
