//! CC session logging — persist session metadata to the unified mando.db.

use std::path::Path;

use mando_types::SessionStatus;
use sqlx::SqlitePool;
use tracing::warn;

pub struct SessionLogEntry<'a> {
    pub session_id: &'a str,
    pub cwd: &'a Path,
    pub model: &'a str,
    pub caller: &'a str,
    pub cost_usd: Option<f64>,
    pub duration_ms: Option<u64>,
    pub resumed: bool,
    pub task_id: &'a str,
    pub status: SessionStatus,
    pub worker_name: &'a str,
}

/// Convert empty string to None, non-empty to Some.
fn non_empty(s: &str) -> Option<&str> {
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

pub async fn log_cc_session(pool: &SqlitePool, entry: &SessionLogEntry<'_>) {
    if let Err(e) = mando_db::queries::sessions::upsert_session(
        pool,
        &mando_db::queries::sessions::SessionUpsert {
            session_id: entry.session_id,
            created_at: &mando_types::now_rfc3339(),
            caller: entry.caller,
            cwd: &entry.cwd.display().to_string(),
            model: entry.model,
            status: entry.status,
            cost_usd: entry.cost_usd,
            duration_ms: entry.duration_ms.map(|d| d as i64),
            resumed: entry.resumed,
            task_id: non_empty(entry.task_id),
            scout_item_id: None,
            worker_name: non_empty(entry.worker_name),
        },
    )
    .await
    {
        warn!(module = "cc", error = %e, "failed to upsert session");
    }
}

pub(crate) async fn log_running_session(
    pool: &SqlitePool,
    session_id: &str,
    cwd: &Path,
    caller: &str,
    worker_name: &str,
    task_id: &str,
    resumed: bool,
) {
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
        },
    )
    .await;
}

pub(crate) async fn log_session_completion(
    pool: &SqlitePool,
    session_id: &str,
    cwd: &str,
    caller: &str,
    worker_name: &str,
    task_id: &str,
    status: SessionStatus,
) {
    let cwd_path = Path::new(cwd);
    log_cc_session(
        pool,
        &SessionLogEntry {
            session_id,
            cwd: cwd_path,
            model: "",
            caller,
            cost_usd: None,
            duration_ms: None,
            resumed: false,
            task_id,
            status,
            worker_name,
        },
    )
    .await;
}

pub(crate) async fn log_cc_result(
    pool: &SqlitePool,
    result: &mando_cc::CcResult,
    cwd: &Path,
    caller: &str,
    task_id: &str,
) {
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
        },
    )
    .await;
}

pub(crate) async fn log_cc_failure(
    pool: &SqlitePool,
    session_id: &str,
    cwd: &Path,
    caller: &str,
    task_id: &str,
) {
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
        },
    )
    .await;
}

pub(crate) async fn log_item_session(
    pool: &SqlitePool,
    item: &mando_types::Task,
    worker_name: &str,
    status: SessionStatus,
) {
    if let Some(ref sid) = item.session_ids.worker {
        let cwd = item.worktree.as_deref().unwrap_or("");
        log_session_completion(
            pool,
            sid,
            cwd,
            "worker",
            worker_name,
            &item.best_id(),
            status,
        )
        .await;
    }
}
