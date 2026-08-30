use std::path::Path;

use anyhow::Result;

use super::codex_app_server::{start_codex_turn, CodexOutputMode};
use super::codex_session::{begin_codex_session, CodexSessionSpec};
use crate::Task;

pub(super) struct CodexRebaseSession {
    pub(super) session_id: String,
    pub(super) pid: crate::Pid,
}

#[tracing::instrument(skip(pool, item, prompt), fields(task_id = item.id, provider = "codex", worker = session_name))]
pub(super) async fn spawn_rebase_worker(
    pool: &sqlx::SqlitePool,
    item: &Task,
    session_name: &str,
    cwd: &Path,
    prompt: &str,
    agent_config: &settings::AgentConfig,
) -> Result<CodexRebaseSession> {
    let started = start_codex_turn(cwd, prompt, None, CodexOutputMode::Text, agent_config).await?;
    let session = begin_codex_session(
        pool,
        started,
        CodexSessionSpec {
            caller: "rebase",
            task_id: item.id,
            project: &item.project,
            worker_name: Some(session_name),
            cwd,
            prompt,
            resumed: false,
            alias: Some(session_name),
            abort_reason: "rebase setup failed",
        },
    )
    .await?;

    Ok(CodexRebaseSession {
        session_id: session.session_id,
        pid: session.pid,
    })
}
