//! CC session logging; persist session metadata to the unified mando.db.

use std::path::Path;

use anyhow::{Context, Result};
use global_types::SessionStatus;
use sqlx::SqlitePool;

pub struct SessionLogEntry<'a> {
    pub session_id: &'a str,
    pub cwd: &'a Path,
    pub model: &'a str,
    pub caller: &'a str,
    pub cost_usd: Option<f64>,
    pub duration_ms: Option<u64>,
    pub resumed: bool,
    pub task_id: Option<i64>,
    pub status: SessionStatus,
    pub worker_name: &'a str,
    pub credential_id: Option<i64>,
    /// Error text from a CC failure (populated on fail paths so the session
    /// row carries the context without re-reading the stream file).
    pub error: Option<&'a str>,
    /// HTTP-like status from the Anthropic API envelope, when the CC run
    /// ended with an `is_error` result.
    pub api_error_status: Option<i64>,
}

/// Convert empty string to None, non-empty to Some.
fn non_empty(s: &str) -> Option<&str> {
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

pub async fn log_cc_session(pool: &SqlitePool, entry: &SessionLogEntry<'_>) -> Result<()> {
    // On resume: pass "" for created_at (preserve original), set resumed_at to now.
    // On initial insert: set created_at to now, leave resumed_at as None.
    let (created_at, resumed_at) = if entry.resumed {
        (String::new(), Some(global_types::now_rfc3339()))
    } else {
        (global_types::now_rfc3339(), None)
    };
    sessions_db::upsert_session(
        pool,
        &sessions_db::SessionUpsert {
            session_id: entry.session_id,
            created_at: &created_at,
            caller: entry.caller,
            cwd: &entry.cwd.display().to_string(),
            model: entry.model,
            status: entry.status,
            cost_usd: entry.cost_usd,
            duration_ms: entry.duration_ms.map(|d| d as i64),
            resumed: entry.resumed,
            task_id: entry.task_id,
            scout_item_id: None,
            worker_name: non_empty(entry.worker_name),
            resumed_at: resumed_at.as_deref(),
            credential_id: entry.credential_id,
            error: entry.error,
            api_error_status: entry.api_error_status,
        },
    )
    .await
    .context("upsert_session")?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn log_running_session(
    pool: &SqlitePool,
    session_id: &str,
    cwd: &Path,
    caller: &str,
    worker_name: &str,
    task_id: Option<i64>,
    resumed: bool,
    credential_id: Option<i64>,
) -> Result<()> {
    log_cc_session(
        pool,
        &SessionLogEntry {
            session_id,
            cwd,
            model: "default",
            caller,
            cost_usd: None,
            duration_ms: None,
            resumed,
            task_id,
            status: SessionStatus::Running,
            worker_name,
            credential_id,
            error: None,
            api_error_status: None,
        },
    )
    .await
}

pub(crate) async fn log_session_completion(
    pool: &SqlitePool,
    session_id: &str,
    _cwd: &str,
    _caller: &str,
    _worker_name: &str,
    _task_id: Option<i64>,
    status: SessionStatus,
) -> Result<()> {
    // Guard: skip cost write if session is already stopped to prevent
    // double-counting under ADD semantics.
    if !sessions_db::is_session_running(pool, session_id).await? {
        return Ok(());
    }

    let stream_path = global_infra::paths::stream_path_for_session(session_id);
    let cost_info = global_claude::get_stream_cost(&stream_path);
    let (cost_usd, duration_ms, num_turns, denials) = match &cost_info {
        Some(info) => (
            info.cost_usd,
            info.duration_ms.map(|d| d as i64),
            info.num_turns,
            info.permission_denials_count,
        ),
        None => (None, None, None, None),
    };

    if let Err(e) = sessions_db::update_session_status_with_cost(
        pool,
        session_id,
        status,
        cost_usd,
        duration_ms,
        num_turns,
    )
    .await
    {
        tracing::warn!(
            module = "headless_cc",
            session_id,
            error = %e,
            "failed to update session cost"
        );
    }

    // Surface permission denials and per-model cost in obs so escalation
    // signals don't sit silently in the stream file. A non-zero denial count
    // usually means the worker hit a guard rail — captain uses this plus the
    // cost breakdown to reason about rework vs abandon decisions.
    if denials.unwrap_or(0) > 0
        || cost_info
            .as_ref()
            .and_then(|i| i.model_usage.as_ref())
            .is_some()
    {
        let model_usage_log = cost_info
            .as_ref()
            .and_then(|i| i.model_usage.as_ref())
            .map(|v| serde_json::to_string(v).unwrap_or_default())
            .unwrap_or_default();
        tracing::info!(
            module = "headless_cc",
            session_id,
            permission_denials = ?denials,
            model_usage = %model_usage_log,
            "session completed with non-zero denials or per-model cost breakdown"
        );
    }

    Ok(())
}

pub(crate) async fn log_cc_result(
    pool: &SqlitePool,
    result: &global_claude::CcResult<serde_json::Value>,
    cwd: &Path,
    caller: &str,
    task_id: Option<i64>,
) -> Result<()> {
    log_cc_session(
        pool,
        &SessionLogEntry {
            session_id: &result.session_id,
            cwd,
            model: "",
            caller,
            cost_usd: result.cost_usd,
            duration_ms: result.duration_ms,
            resumed: false,
            task_id,
            status: SessionStatus::Stopped,
            worker_name: "",
            credential_id: None,
            error: None,
            api_error_status: None,
        },
    )
    .await
}

pub async fn log_cc_failure(
    pool: &SqlitePool,
    session_id: &str,
    cwd: &Path,
    caller: &str,
    task_id: Option<i64>,
    error: Option<&str>,
    api_error_status: Option<i64>,
) -> Result<()> {
    log_cc_session(
        pool,
        &SessionLogEntry {
            session_id,
            cwd,
            model: "",
            caller,
            cost_usd: None,
            duration_ms: None,
            resumed: false,
            task_id,
            status: SessionStatus::Failed,
            worker_name: "",
            credential_id: None,
            error,
            api_error_status,
        },
    )
    .await
}

pub(crate) async fn log_item_session(
    pool: &SqlitePool,
    item: &crate::Task,
    worker_name: &str,
    status: SessionStatus,
) -> Result<()> {
    if let Some(ref sid) = item.session_ids.worker {
        let cwd = item.worktree.as_deref().unwrap_or("");
        log_session_completion(pool, sid, cwd, "worker", worker_name, Some(item.id), status)
            .await?;
    }
    Ok(())
}
