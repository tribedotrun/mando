//! Unified session queries — all CC sessions in one table.

use std::collections::HashMap;

use anyhow::Result;
use sqlx::SqlitePool;

use mando_types::SessionStatus;

use crate::caller::{CallerGroup, SessionCaller};

/// A session row from the unified sessions table.
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct SessionRow {
    pub session_id: String,
    pub created_at: String,
    pub caller: String,
    pub cwd: String,
    pub model: String,
    pub status: String,
    pub cost_usd: Option<f64>,
    pub duration_ms: Option<i64>,
    pub resumed: i64,
    pub turn_count: i64,
    pub task_id: Option<String>,
    pub scout_item_id: Option<i64>,
    pub worker_name: Option<String>,
}

impl SessionRow {
    /// Parse the caller string into the enum.
    pub fn parsed_caller(&self) -> Option<SessionCaller> {
        SessionCaller::parse(&self.caller)
    }

    /// Get the display group for this session.
    pub fn group(&self) -> Option<CallerGroup> {
        self.parsed_caller().map(|c| c.group())
    }

    /// Parse the status string into the enum.
    pub fn parsed_status(&self) -> SessionStatus {
        self.status.parse().unwrap_or(SessionStatus::Stopped)
    }
}

/// Input for upserting a session.
pub struct SessionUpsert<'a> {
    pub session_id: &'a str,
    pub created_at: &'a str,
    pub caller: &'a str,
    pub cwd: &'a str,
    pub model: &'a str,
    pub status: SessionStatus,
    pub cost_usd: Option<f64>,
    pub duration_ms: Option<i64>,
    pub resumed: bool,
    pub task_id: Option<&'a str>,
    pub scout_item_id: Option<i64>,
    pub worker_name: Option<&'a str>,
}

/// Upsert a session with cumulative cost tracking.
/// On conflict: cost and duration are ADDED (not replaced), turn_count increments,
/// resumed latches to true, other fields use "non-empty wins" logic.
pub async fn upsert_session(pool: &SqlitePool, input: &SessionUpsert<'_>) -> Result<()> {
    sqlx::query(
        "INSERT INTO cc_sessions (session_id, created_at, caller, cwd, model, status,
            cost_usd, duration_ms, resumed, turn_count, task_id, scout_item_id, worker_name)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 1, ?10, ?11, ?12)
        ON CONFLICT(session_id) DO UPDATE SET
            created_at = CASE WHEN excluded.created_at != '' THEN excluded.created_at ELSE cc_sessions.created_at END,
            caller = CASE WHEN excluded.caller != '' THEN excluded.caller ELSE cc_sessions.caller END,
            cwd = CASE WHEN excluded.cwd != '' THEN excluded.cwd ELSE cc_sessions.cwd END,
            model = CASE WHEN excluded.model != '' THEN excluded.model ELSE cc_sessions.model END,
            status = CASE WHEN excluded.status != '' THEN excluded.status ELSE cc_sessions.status END,
            cost_usd = CASE
                WHEN excluded.cost_usd IS NOT NULL AND cc_sessions.cost_usd IS NOT NULL
                    THEN cc_sessions.cost_usd + excluded.cost_usd
                WHEN excluded.cost_usd IS NOT NULL THEN excluded.cost_usd
                ELSE cc_sessions.cost_usd
            END,
            duration_ms = CASE
                WHEN excluded.duration_ms IS NOT NULL AND cc_sessions.duration_ms IS NOT NULL
                    THEN cc_sessions.duration_ms + excluded.duration_ms
                WHEN excluded.duration_ms IS NOT NULL THEN excluded.duration_ms
                ELSE cc_sessions.duration_ms
            END,
            resumed = MAX(cc_sessions.resumed, excluded.resumed),
            turn_count = CASE
                WHEN excluded.cost_usd IS NOT NULL THEN cc_sessions.turn_count + 1
                ELSE cc_sessions.turn_count
            END,
            task_id = COALESCE(excluded.task_id, cc_sessions.task_id),
            scout_item_id = COALESCE(excluded.scout_item_id, cc_sessions.scout_item_id),
            worker_name = COALESCE(excluded.worker_name, cc_sessions.worker_name)",
    )
    .bind(input.session_id)
    .bind(input.created_at)
    .bind(input.caller)
    .bind(input.cwd)
    .bind(input.model)
    .bind(input.status.as_str())
    .bind(input.cost_usd)
    .bind(input.duration_ms)
    .bind(input.resumed as i64)
    .bind(input.task_id)
    .bind(input.scout_item_id)
    .bind(input.worker_name)
    .execute(pool)
    .await?;
    Ok(())
}

/// Category counts grouped by CallerGroup for the UI.
pub async fn category_counts(pool: &SqlitePool) -> Result<HashMap<String, usize>> {
    let rows: Vec<(String, i64)> =
        sqlx::query_as("SELECT caller, COUNT(*) FROM cc_sessions GROUP BY caller")
            .fetch_all(pool)
            .await?;

    let mut group_counts: HashMap<String, usize> = HashMap::new();
    for (caller_str, count) in rows {
        let group_name = SessionCaller::parse(&caller_str)
            .map(|c| c.group().as_str().to_string())
            .unwrap_or_else(|| caller_str.clone());
        *group_counts.entry(group_name).or_default() += count as usize;
    }
    Ok(group_counts)
}

/// Paginated session listing with optional group filter.
pub async fn list_sessions(
    pool: &SqlitePool,
    page: usize,
    per_page: usize,
    group: Option<&str>,
) -> Result<(Vec<SessionRow>, usize)> {
    let per_page = if per_page == 0 { 50 } else { per_page };
    let offset = page.saturating_sub(1) * per_page;

    // Build the list of caller strings that belong to this group.
    let caller_filter: Option<Vec<&str>> = group.map(|g| {
        SessionCaller::all()
            .iter()
            .filter(|c| c.group().as_str() == g)
            .map(|c| c.as_str())
            .collect()
    });

    let (rows, total) = match &caller_filter {
        None => {
            let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cc_sessions")
                .fetch_one(pool)
                .await?;
            let rows: Vec<SessionRow> = sqlx::query_as(
                "SELECT session_id, created_at, caller, cwd, model, status,
                        cost_usd, duration_ms, resumed, turn_count,
                        task_id, scout_item_id, worker_name
                 FROM cc_sessions ORDER BY created_at DESC LIMIT ? OFFSET ?",
            )
            .bind(per_page as i64)
            .bind(offset as i64)
            .fetch_all(pool)
            .await?;
            (rows, total as usize)
        }
        Some(callers) if callers.is_empty() => (Vec::new(), 0),
        Some(callers) => {
            // Build dynamic IN clause.
            let placeholders: String = callers.iter().map(|_| "?").collect::<Vec<_>>().join(",");

            let count_sql =
                format!("SELECT COUNT(*) FROM cc_sessions WHERE caller IN ({placeholders})");
            let mut q = sqlx::query_scalar::<_, i64>(&count_sql);
            for c in callers {
                q = q.bind(*c);
            }
            let total: i64 = q.fetch_one(pool).await?;

            let select_sql = format!(
                "SELECT session_id, created_at, caller, cwd, model, status,
                        cost_usd, duration_ms, resumed, turn_count,
                        task_id, scout_item_id, worker_name
                 FROM cc_sessions WHERE caller IN ({placeholders})
                 ORDER BY created_at DESC LIMIT ? OFFSET ?"
            );
            let mut q = sqlx::query_as::<_, SessionRow>(&select_sql);
            for c in callers {
                q = q.bind(*c);
            }
            q = q.bind(per_page as i64).bind(offset as i64);
            let rows = q.fetch_all(pool).await?;
            (rows, total as usize)
        }
    };

    Ok((rows, total))
}

/// List all sessions linked to a task.
pub async fn list_sessions_for_task(pool: &SqlitePool, task_id: &str) -> Result<Vec<SessionRow>> {
    let rows: Vec<SessionRow> = sqlx::query_as(
        "SELECT session_id, created_at, caller, cwd, model, status,
                cost_usd, duration_ms, resumed, turn_count,
                task_id, scout_item_id, worker_name
         FROM cc_sessions WHERE task_id = ? ORDER BY created_at DESC",
    )
    .bind(task_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// List all sessions linked to a scout item.
pub async fn list_sessions_for_scout_item(
    pool: &SqlitePool,
    item_id: i64,
) -> Result<Vec<SessionRow>> {
    let rows: Vec<SessionRow> = sqlx::query_as(
        "SELECT session_id, created_at, caller, cwd, model, status,
                cost_usd, duration_ms, resumed, turn_count,
                task_id, scout_item_id, worker_name
         FROM cc_sessions WHERE scout_item_id = ? ORDER BY created_at DESC",
    )
    .bind(item_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Delete all sessions linked to a task.
pub async fn delete_sessions_for_task(pool: &SqlitePool, task_id: &str) -> Result<u64> {
    let result = sqlx::query("DELETE FROM cc_sessions WHERE task_id = ?")
        .bind(task_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

/// Get the cwd for a session.
pub async fn session_cwd(pool: &SqlitePool, session_id: &str) -> Result<Option<String>> {
    let row: Option<(String,)> = sqlx::query_as("SELECT cwd FROM cc_sessions WHERE session_id = ?")
        .bind(session_id)
        .fetch_optional(pool)
        .await?;
    Ok(row.and_then(|(cwd,)| if cwd.is_empty() { None } else { Some(cwd) }))
}

/// Total cost across all sessions.
pub async fn total_session_cost(pool: &SqlitePool) -> Result<f64> {
    let cost: f64 = sqlx::query_scalar("SELECT COALESCE(SUM(cost_usd), 0.0) FROM cc_sessions")
        .fetch_one(pool)
        .await?;
    Ok(cost)
}

/// Update only the status of a session (targeted update for reconciliation).
pub async fn update_session_status(
    pool: &SqlitePool,
    session_id: &str,
    status: SessionStatus,
) -> Result<()> {
    sqlx::query("UPDATE cc_sessions SET status = ? WHERE session_id = ?")
        .bind(status.as_str())
        .bind(session_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// List all sessions currently marked as running (for tick reconciliation).
pub async fn list_running_sessions(pool: &SqlitePool) -> Result<Vec<SessionRow>> {
    let rows: Vec<SessionRow> = sqlx::query_as(
        "SELECT session_id, created_at, caller, cwd, model, status,
                cost_usd, duration_ms, resumed, turn_count,
                task_id, scout_item_id, worker_name
         FROM cc_sessions WHERE status = 'running'",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Db;

    async fn test_pool() -> SqlitePool {
        let db = Db::open_in_memory().await.unwrap();
        db.pool().clone()
    }

    #[tokio::test]
    async fn upsert_and_list() {
        let pool = test_pool().await;
        upsert_session(
            &pool,
            &SessionUpsert {
                session_id: "s1",
                created_at: "2026-03-26T00:00:00Z",
                caller: "worker",
                cwd: "/tmp",
                model: "opus",
                status: SessionStatus::Stopped,
                cost_usd: Some(1.5),
                duration_ms: Some(30000),
                resumed: false,
                task_id: Some("task-1"),
                scout_item_id: None,
                worker_name: Some("main-v1"),
            },
        )
        .await
        .unwrap();

        let (rows, total) = list_sessions(&pool, 1, 50, None).await.unwrap();
        assert_eq!(total, 1);
        assert_eq!(rows[0].session_id, "s1");
        assert_eq!(rows[0].cost_usd, Some(1.5));
        assert_eq!(rows[0].turn_count, 1);
    }

    #[tokio::test]
    async fn cumulative_cost_on_upsert() {
        let pool = test_pool().await;
        // First turn
        upsert_session(
            &pool,
            &SessionUpsert {
                session_id: "s1",
                created_at: "2026-03-26T00:00:00Z",
                caller: "worker",
                cwd: "/tmp",
                model: "opus",
                status: SessionStatus::Running,
                cost_usd: Some(1.0),
                duration_ms: Some(10000),
                resumed: false,
                task_id: Some("task-1"),
                scout_item_id: None,
                worker_name: None,
            },
        )
        .await
        .unwrap();

        // Second turn (resume)
        upsert_session(
            &pool,
            &SessionUpsert {
                session_id: "s1",
                created_at: "",
                caller: "worker",
                cwd: "",
                model: "",
                status: SessionStatus::Stopped,
                cost_usd: Some(0.5),
                duration_ms: Some(5000),
                resumed: true,
                task_id: None,
                scout_item_id: None,
                worker_name: None,
            },
        )
        .await
        .unwrap();

        let (rows, _) = list_sessions(&pool, 1, 50, None).await.unwrap();
        assert_eq!(rows[0].cost_usd, Some(1.5)); // 1.0 + 0.5
        assert_eq!(rows[0].duration_ms, Some(15000)); // 10000 + 5000
        assert_eq!(rows[0].turn_count, 2);
        assert_eq!(rows[0].resumed, 1);
    }

    #[tokio::test]
    async fn category_counts_group_by_caller_group() {
        let pool = test_pool().await;
        upsert_session(
            &pool,
            &SessionUpsert {
                session_id: "s1",
                created_at: "2026-03-26T00:00:00Z",
                caller: "scout-process",
                cwd: "",
                model: "",
                status: SessionStatus::Stopped,
                cost_usd: Some(0.5),
                duration_ms: None,
                resumed: false,
                task_id: None,
                scout_item_id: Some(1),
                worker_name: None,
            },
        )
        .await
        .unwrap();
        upsert_session(
            &pool,
            &SessionUpsert {
                session_id: "s2",
                created_at: "2026-03-26T00:00:01Z",
                caller: "scout-article",
                cwd: "",
                model: "",
                status: SessionStatus::Stopped,
                cost_usd: Some(0.7),
                duration_ms: None,
                resumed: false,
                task_id: None,
                scout_item_id: Some(1),
                worker_name: None,
            },
        )
        .await
        .unwrap();
        upsert_session(
            &pool,
            &SessionUpsert {
                session_id: "s3",
                created_at: "2026-03-26T00:00:02Z",
                caller: "worker",
                cwd: "/tmp",
                model: "opus",
                status: SessionStatus::Stopped,
                cost_usd: Some(2.0),
                duration_ms: None,
                resumed: false,
                task_id: Some("t1"),
                scout_item_id: None,
                worker_name: Some("w1"),
            },
        )
        .await
        .unwrap();

        let counts = category_counts(&pool).await.unwrap();
        assert_eq!(counts.get("scout"), Some(&2));
        assert_eq!(counts.get("workers"), Some(&1));
    }

    #[tokio::test]
    async fn filter_by_group() {
        let pool = test_pool().await;
        upsert_session(
            &pool,
            &SessionUpsert {
                session_id: "s1",
                created_at: "2026-03-26T00:00:00Z",
                caller: "scout-process",
                cwd: "",
                model: "",
                status: SessionStatus::Stopped,
                cost_usd: None,
                duration_ms: None,
                resumed: false,
                task_id: None,
                scout_item_id: Some(1),
                worker_name: None,
            },
        )
        .await
        .unwrap();
        upsert_session(
            &pool,
            &SessionUpsert {
                session_id: "s2",
                created_at: "2026-03-26T00:00:01Z",
                caller: "worker",
                cwd: "/tmp",
                model: "",
                status: SessionStatus::Stopped,
                cost_usd: None,
                duration_ms: None,
                resumed: false,
                task_id: Some("t1"),
                scout_item_id: None,
                worker_name: None,
            },
        )
        .await
        .unwrap();

        let (rows, total) = list_sessions(&pool, 1, 50, Some("scout")).await.unwrap();
        assert_eq!(total, 1);
        assert_eq!(rows[0].caller, "scout-process");
    }

    #[tokio::test]
    async fn list_by_scout_item() {
        let pool = test_pool().await;
        upsert_session(
            &pool,
            &SessionUpsert {
                session_id: "s1",
                created_at: "2026-03-26T00:00:00Z",
                caller: "scout-process",
                cwd: "",
                model: "",
                status: SessionStatus::Stopped,
                cost_usd: None,
                duration_ms: None,
                resumed: false,
                task_id: None,
                scout_item_id: Some(42),
                worker_name: None,
            },
        )
        .await
        .unwrap();
        upsert_session(
            &pool,
            &SessionUpsert {
                session_id: "s2",
                created_at: "2026-03-26T00:00:01Z",
                caller: "scout-article",
                cwd: "",
                model: "",
                status: SessionStatus::Stopped,
                cost_usd: None,
                duration_ms: None,
                resumed: false,
                task_id: None,
                scout_item_id: Some(42),
                worker_name: None,
            },
        )
        .await
        .unwrap();
        upsert_session(
            &pool,
            &SessionUpsert {
                session_id: "s3",
                created_at: "2026-03-26T00:00:02Z",
                caller: "worker",
                cwd: "",
                model: "",
                status: SessionStatus::Stopped,
                cost_usd: None,
                duration_ms: None,
                resumed: false,
                task_id: None,
                scout_item_id: None,
                worker_name: None,
            },
        )
        .await
        .unwrap();

        let rows = list_sessions_for_scout_item(&pool, 42).await.unwrap();
        assert_eq!(rows.len(), 2);
    }
}
