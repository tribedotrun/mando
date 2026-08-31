//! Provider-neutral rebase phase runner.

use anyhow::Result;

use crate::{Pid, Task};

pub(crate) struct RebaseSession {
    pub(crate) session_id: String,
    pub(crate) pid: Pid,
}

#[tracing::instrument(skip_all, fields(provider = %item.provider.as_str(), task_id = item.id, worker = session_name))]
pub(super) async fn spawn(
    item: &Task,
    pool: &sqlx::SqlitePool,
    session_name: &str,
    cwd: &std::path::Path,
    prompt: &str,
    model: &str,
    workflow: &settings::CaptainWorkflow,
) -> Result<RebaseSession> {
    let session = super::agent_runtime::Adapter::for_task(item)?
        .start_rebase(item, pool, session_name, cwd, prompt, model, workflow)
        .await?;
    Ok(RebaseSession {
        session_id: session.session_id,
        pid: session.pid,
    })
}
