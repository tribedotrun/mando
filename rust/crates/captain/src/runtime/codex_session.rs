use std::path::{Path, PathBuf};

use anyhow::Result;
use serde_json::json;

use super::codex_app_server::{abort_started_turn, watch_codex_turn, CodexStarted};
use super::codex_stream::{append_jsonl, CodexStreamLine};

/// A Codex turn that has been recorded and handed to the stream watcher.
pub(super) struct CodexSession {
    pub(super) session_id: String,
    pub(super) pid: crate::Pid,
    pub(super) stream_path: PathBuf,
}

/// The per-callsite parameters of an otherwise identical Codex session setup.
pub(super) struct CodexSessionSpec<'a> {
    pub(super) caller: &'a str,
    pub(super) task_id: i64,
    pub(super) project: &'a str,
    /// `None` records the session with no worker name and writes an empty one
    /// into the stream meta.
    pub(super) worker_name: Option<&'a str>,
    pub(super) cwd: &'a Path,
    pub(super) prompt: &'a str,
    /// Resumed sessions keep their original `created_at` and get a fresh
    /// `resumed_at`; fresh ones get the reverse.
    pub(super) resumed: bool,
    /// Second pid-registry key registered alongside the thread id, and
    /// unregistered again if setup fails.
    pub(super) alias: Option<&'a str>,
    pub(super) abort_reason: &'static str,
}

/// Seed the stream file, register the pid, record the session row and stream
/// meta, then start the watcher.
///
/// Any failure aborts the started turn, so a half-recorded session never
/// leaves a Codex thread running unwatched.
#[tracing::instrument(skip_all, fields(provider = "codex", task_id = spec.task_id, caller = spec.caller))]
pub(super) async fn begin_codex_session(
    pool: &sqlx::SqlitePool,
    started: CodexStarted,
    spec: CodexSessionSpec<'_>,
) -> Result<CodexSession> {
    let stream_path =
        global_infra::paths::codex_derived_stream_path_for_session(&started.thread_id);
    if let Err(e) = record_session(pool, &started, &spec, &stream_path).await {
        abort_started_turn(started, spec.alias, spec.abort_reason).await;
        return Err(e);
    }

    let session_id = started.thread_id.clone();
    let pid = started.pid;
    watch_codex_turn(started, pool.clone(), stream_path.clone());
    Ok(CodexSession {
        session_id,
        pid,
        stream_path,
    })
}

async fn record_session(
    pool: &sqlx::SqlitePool,
    started: &CodexStarted,
    spec: &CodexSessionSpec<'_>,
    stream_path: &Path,
) -> Result<()> {
    let cwd = spec.cwd.display().to_string();
    append_jsonl(
        stream_path,
        CodexStreamLine(json!({
            "type": "system",
            "subtype": "init",
            "session_id": &started.thread_id,
            "provider": "codex",
            "cwd": cwd,
        })),
    )
    .await?;
    append_jsonl(
        stream_path,
        CodexStreamLine(json!({
            "type": "user",
            "message": {"content": [{"type": "text", "text": spec.prompt}]},
        })),
    )
    .await?;

    if let Some(alias) = spec.alias {
        crate::io::pid_registry::register(alias, started.pid)?;
    }
    crate::io::pid_registry::register(&started.thread_id, started.pid)?;

    let resumed_at = spec.resumed.then(global_types::now_rfc3339);
    let created_at = if spec.resumed {
        String::new()
    } else {
        global_types::now_rfc3339()
    };
    sessions_db::upsert_session(
        pool,
        &sessions_db::SessionUpsert {
            provider: global_types::TaskProvider::Codex,
            session_id: &started.thread_id,
            created_at: &created_at,
            caller: spec.caller,
            cwd: &cwd,
            model: &started.model,
            status: global_types::SessionStatus::Running,
            cost_usd: None,
            duration_ms: None,
            resumed: spec.resumed,
            task_id: Some(spec.task_id),
            scout_item_id: None,
            worker_name: spec.worker_name,
            resumed_at: resumed_at.as_deref(),
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
            caller: spec.caller,
            task_id: &spec.task_id.to_string(),
            worker_name: spec.worker_name.unwrap_or(""),
            project: spec.project,
            cwd: &cwd,
        },
        "running",
    );
    Ok(())
}
