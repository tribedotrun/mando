use std::path::{Path, PathBuf};

use anyhow::Result;

use super::codex_app_server::{start_codex_turn, CodexOutputMode};
use super::codex_session::{begin_codex_session, CodexSessionSpec};
use crate::{Pid, Task};

#[tracing::instrument(skip(pool, item, prompt), fields(task_id = item.id, session_id, provider = "codex"))]
pub(crate) async fn resume_worker(
    pool: &sqlx::SqlitePool,
    item: &Task,
    worker_name: &str,
    cwd: &Path,
    prompt: &str,
    session_id: &str,
    agent_config: &settings::AgentConfig,
) -> Result<(Pid, PathBuf, String)> {
    let started = start_codex_turn(
        cwd,
        prompt,
        Some(session_id),
        CodexOutputMode::Text,
        agent_config,
    )
    .await?;
    let session = begin_codex_session(
        pool,
        started,
        CodexSessionSpec {
            caller: "worker",
            task_id: item.id,
            project: &item.project,
            worker_name: Some(worker_name),
            cwd,
            prompt,
            resumed: true,
            alias: None,
            abort_reason: "worker resume setup failed",
        },
    )
    .await?;

    Ok((session.pid, session.stream_path, session.session_id))
}
