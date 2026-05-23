use std::path::{Path, PathBuf};

use anyhow::Result;
use serde_json::json;

use super::codex_app_server::{start_codex_turn, watch_codex_turn, CodexOutputMode};
use super::codex_stream::{append_jsonl, CodexStreamLine};
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
        crate::io::pid_registry::register(&started.thread_id, started.pid)?;
        let resumed_at = global_types::now_rfc3339();
        sessions_db::upsert_session(
            pool,
            &sessions_db::SessionUpsert {
                provider: global_types::TaskProvider::Codex,
                session_id: &started.thread_id,
                created_at: "",
                caller: "worker",
                cwd: &cwd.display().to_string(),
                model: &started.model,
                status: global_types::SessionStatus::Running,
                cost_usd: None,
                duration_ms: None,
                resumed: true,
                task_id: Some(item.id),
                scout_item_id: None,
                worker_name: Some(worker_name),
                resumed_at: Some(&resumed_at),
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
                caller: "worker",
                task_id: &item.id.to_string(),
                worker_name,
                project: &item.project,
                cwd: &cwd.display().to_string(),
            },
            "running",
        );
        Ok(())
    }
    .await;
    if let Err(e) = setup_result {
        super::codex_app_server::abort_started_turn(started, None, "worker resume setup failed")
            .await;
        return Err(e);
    }
    let pid = started.pid;
    let thread_id = started.thread_id.clone();
    watch_codex_turn(started, pool.clone(), stream_path.clone());
    Ok((pid, stream_path, thread_id))
}
