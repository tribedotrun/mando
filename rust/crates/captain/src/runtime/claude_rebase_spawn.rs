//! Claude adapter for rebase worker sessions.

use anyhow::Result;

use crate::{Pid, Task};

pub(super) struct ClaudeRebaseSession {
    pub(super) session_id: String,
    pub(super) pid: Pid,
}

#[tracing::instrument(skip_all, fields(provider = "claude", task_id = item.id, worker = session_name))]
pub(super) async fn spawn_rebase_worker(
    item: &Task,
    pool: &sqlx::SqlitePool,
    session_name: &str,
    cwd: &std::path::Path,
    prompt: &str,
    model: &str,
) -> Result<ClaudeRebaseSession> {
    let session_id = global_infra::uuid::Uuid::v4().to_string();

    // Pick credential so the rebase worker participates in load balancing.
    let credential = super::tick_spawn::pick_credential(pool, None).await;
    let cred_id = global_claude::credential_id(&credential);
    let mut env: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    if let Some((_id, token)) = &credential {
        env.insert("CLAUDE_CODE_OAUTH_TOKEN".into(), token.clone());
    }

    let (pid, _) =
        crate::io::process_manager::spawn_worker_process(prompt, cwd, model, &session_id, &env)
            .await?;

    // Register under both session_name (for reap_dead_rebase_workers
    // lifecycle) and session_id (for session reconciler + terminator).
    if let Err(e) = crate::io::pid_registry::register(session_name, pid) {
        tracing::warn!(module = "captain", worker = %session_name, %e, "pid_registry register failed");
    }
    if let Err(e) = crate::io::pid_registry::register(&session_id, pid) {
        tracing::warn!(module = "captain", session_id = %session_id, %e, "pid_registry register (session_id) failed");
    }
    // Log "running" session entry so the UI shows it immediately.
    if let Err(e) = crate::io::headless_cc::log_running_session(
        pool,
        &session_id,
        cwd,
        "rebase",
        session_name,
        Some(item.id),
        false,
        cred_id,
    )
    .await
    {
        tracing::warn!(
            module = "captain",
            session_id = %session_id,
            error = %e,
            "failed to log rebase session"
        );
    }

    Ok(ClaudeRebaseSession { session_id, pid })
}
