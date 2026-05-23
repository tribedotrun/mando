use std::path::Path;

use anyhow::Result;
use serde_json::json;

use super::codex_app_server::{start_codex_turn, watch_codex_turn, CodexOutputMode};
use super::codex_stream::{append_jsonl, CodexStreamLine};
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
    let stream_path =
        global_infra::paths::codex_derived_stream_path_for_session(&started.thread_id);
    let setup_result: Result<()> = async {
        append_jsonl(
            &stream_path,
            CodexStreamLine(json!({
                "type": "system",
                "subtype": "init",
                "session_id": &started.thread_id,
                "provider": "codex",
                "cwd": cwd.display().to_string(),
            })),
        )
        .await?;
        append_jsonl(
            &stream_path,
            CodexStreamLine(json!({
                "type": "user",
                "message": {"content": [{"type": "text", "text": prompt}]},
            })),
        )
        .await?;

        crate::io::pid_registry::register(session_name, started.pid)?;
        crate::io::pid_registry::register(&started.thread_id, started.pid)?;
        sessions_db::upsert_session(
            pool,
            &sessions_db::SessionUpsert {
                provider: global_types::TaskProvider::Codex,
                session_id: &started.thread_id,
                created_at: &global_types::now_rfc3339(),
                caller: "rebase",
                cwd: &cwd.display().to_string(),
                model: &started.model,
                status: global_types::SessionStatus::Running,
                cost_usd: None,
                duration_ms: None,
                resumed: false,
                task_id: Some(item.id),
                scout_item_id: None,
                worker_name: Some(session_name),
                resumed_at: None,
                credential_id: None,
                error: None,
                api_error_status: None,
            },
        )
        .await?;

        global_claude::write_stream_meta_at(
            &global_infra::paths::codex_derived_stream_meta_path_for_session(&started.thread_id),
            &global_claude::SessionMeta {
                session_id: &started.thread_id,
                caller: "rebase",
                task_id: &item.id.to_string(),
                worker_name: session_name,
                project: &item.project,
                cwd: &cwd.display().to_string(),
            },
            "running",
        );
        Ok(())
    }
    .await;
    if let Err(e) = setup_result {
        super::codex_app_server::abort_started_turn(
            started,
            Some(session_name),
            "rebase setup failed",
        )
        .await;
        return Err(e);
    }

    let session_id = started.thread_id.clone();
    let pid = started.pid;
    watch_codex_turn(started, pool.clone(), stream_path);

    Ok(CodexRebaseSession { session_id, pid })
}
