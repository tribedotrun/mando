//! Unified session queries — all CC sessions in one table.

use std::collections::HashMap;

use anyhow::Result;
use sqlx::SqlitePool;

use global_types::SessionStatus;

use crate::{CallerGroup, SessionCaller};

/// Column list for SessionRow queries — single source of truth.
const SELECT_COLS: &str = "\
    session_id, created_at, caller, cwd, model, status, \
    cost_usd, duration_ms, resumed, turn_count, \
    task_id, scout_item_id, worker_name, resumed_at, credential_id";

fn select_sessions_sql() -> &'static str {
    static SQL: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    SQL.get_or_init(|| format!("SELECT {SELECT_COLS} FROM cc_sessions"))
}

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
    pub task_id: Option<i64>,
    pub scout_item_id: Option<i64>,
    pub worker_name: Option<String>,
    pub resumed_at: Option<String>,
    pub credential_id: Option<i64>,
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
    pub task_id: Option<i64>,
    pub scout_item_id: Option<i64>,
    pub worker_name: Option<&'a str>,
    pub resumed_at: Option<&'a str>,
    pub credential_id: Option<i64>,
}

/// Upsert a session with cumulative cost tracking.
/// On conflict: cost and duration are ADDED (not replaced), turn_count increments,
/// resumed latches to true, other fields use "non-empty wins" logic.
pub async fn upsert_session(pool: &SqlitePool, input: &SessionUpsert<'_>) -> Result<()> {
    sqlx::query(
        "INSERT INTO cc_sessions (session_id, created_at, caller, cwd, model, status,
            cost_usd, duration_ms, resumed, turn_count, task_id, scout_item_id, worker_name,
            resumed_at, credential_id)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 0, ?10, ?11, ?12, ?13, ?14)
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
            worker_name = COALESCE(excluded.worker_name, cc_sessions.worker_name),
            resumed_at = CASE WHEN excluded.resumed_at IS NOT NULL THEN excluded.resumed_at ELSE cc_sessions.resumed_at END,
            credential_id = COALESCE(excluded.credential_id, cc_sessions.credential_id)",
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
    .bind(input.resumed_at)
    .bind(input.credential_id)
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

/// Paginated session listing with optional group and status filters.
pub async fn list_sessions(
    pool: &SqlitePool,
    page: usize,
    per_page: usize,
    group: Option<&str>,
    status: Option<&str>,
) -> Result<(Vec<SessionRow>, usize)> {
    let per_page = if per_page == 0 { 50 } else { per_page };
    let offset = page.saturating_sub(1) * per_page;

    // Accumulate WHERE conditions and bound parameters.
    let mut where_conditions: Vec<String> = Vec::new();
    let mut params: Vec<String> = Vec::new();

    // Build caller filter for the requested group.
    // Uses exact matches for canonical callers and LIKE patterns for callers
    // that embed IDs in their keys (e.g. "parse-todos-{uuid}", "task-ask:{id}").
    // This keeps the parameter count bounded by the enum size, not DB cardinality.
    if let Some(g) = group {
        let group_callers: Vec<&SessionCaller> = SessionCaller::all()
            .iter()
            .filter(|c| c.group().as_str() == g)
            .collect();

        let mut caller_conditions: Vec<String> = Vec::new();

        // Exact matches for canonical caller names.
        let exact: Vec<&str> = group_callers.iter().map(|c| c.as_str()).collect();
        if !exact.is_empty() {
            let ph: String = exact.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            caller_conditions.push(format!("caller IN ({ph})"));
            params.extend(exact.iter().map(|s| (*s).to_string()));
        }

        // LIKE patterns for callers that use key-embedded IDs.
        for c in &group_callers {
            if let Some(prefix) = c.like_prefix() {
                caller_conditions.push("caller LIKE ?".to_string());
                params.push(prefix.to_string());
            }
        }

        if caller_conditions.is_empty() {
            // Unknown group -- return no results.
            return Ok((Vec::new(), 0));
        }
        where_conditions.push(format!("({})", caller_conditions.join(" OR ")));
    }

    // Add status filter if requested.
    if let Some(s) = status {
        where_conditions.push("status = ?".to_string());
        params.push(s.to_string());
    }

    let (rows, total) = if where_conditions.is_empty() {
        let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cc_sessions")
            .fetch_one(pool)
            .await?;
        let sql = format!(
            "{} ORDER BY created_at DESC LIMIT ? OFFSET ?",
            select_sessions_sql()
        );
        let rows: Vec<SessionRow> = sqlx::query_as(&sql)
            .bind(per_page as i64)
            .bind(offset as i64)
            .fetch_all(pool)
            .await?;
        (rows, total as usize)
    } else {
        let where_clause = where_conditions.join(" AND ");
        let count_sql = format!("SELECT COUNT(*) FROM cc_sessions WHERE {where_clause}");
        let mut q = sqlx::query_scalar::<_, i64>(&count_sql);
        for p in &params {
            q = q.bind(p.as_str());
        }
        let total: i64 = q.fetch_one(pool).await?;

        let select_sql = format!(
            "{} WHERE {where_clause} ORDER BY created_at DESC LIMIT ? OFFSET ?",
            select_sessions_sql()
        );
        let mut q = sqlx::query_as::<_, SessionRow>(&select_sql);
        for p in &params {
            q = q.bind(p.as_str());
        }
        q = q.bind(per_page as i64).bind(offset as i64);
        let rows = q.fetch_all(pool).await?;
        (rows, total as usize)
    };

    Ok((rows, total))
}

/// List all sessions linked to a task.
pub async fn list_sessions_for_task(pool: &SqlitePool, task_id: i64) -> Result<Vec<SessionRow>> {
    let sql = format!(
        "{} WHERE task_id = ? ORDER BY created_at DESC",
        select_sessions_sql()
    );
    let rows: Vec<SessionRow> = sqlx::query_as(&sql).bind(task_id).fetch_all(pool).await?;
    Ok(rows)
}

/// List all sessions linked to a scout item.
pub async fn list_sessions_for_scout_item(
    pool: &SqlitePool,
    item_id: i64,
) -> Result<Vec<SessionRow>> {
    let sql = format!(
        "{} WHERE scout_item_id = ? ORDER BY created_at DESC",
        select_sessions_sql()
    );
    let rows: Vec<SessionRow> = sqlx::query_as(&sql).bind(item_id).fetch_all(pool).await?;
    Ok(rows)
}

/// Delete all sessions linked to a task.
pub async fn delete_sessions_for_task(pool: &SqlitePool, task_id: i64) -> Result<u64> {
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

/// Update status and accumulate cost/duration from stream data.
///
/// Cost and duration use ADD semantics to accumulate across resume cycles.
/// Callers must guard against double-calling for the same segment (e.g.
/// `terminate_session` checks `is_session_running` before calling).
pub async fn update_session_status_with_cost(
    pool: &SqlitePool,
    session_id: &str,
    status: SessionStatus,
    cost_usd: Option<f64>,
    duration_ms: Option<i64>,
    num_turns: Option<i64>,
) -> Result<()> {
    sqlx::query(
        "UPDATE cc_sessions SET
            status = ?1,
            cost_usd = CASE
                WHEN ?2 IS NOT NULL AND cost_usd IS NOT NULL THEN cost_usd + ?2
                WHEN ?2 IS NOT NULL THEN ?2
                ELSE cost_usd
            END,
            duration_ms = CASE
                WHEN ?3 IS NOT NULL AND duration_ms IS NOT NULL THEN duration_ms + ?3
                WHEN ?3 IS NOT NULL THEN ?3
                ELSE duration_ms
            END,
            turn_count = CASE
                WHEN ?4 IS NOT NULL THEN turn_count + ?4
                ELSE turn_count
            END
         WHERE session_id = ?5",
    )
    .bind(status.as_str())
    .bind(cost_usd)
    .bind(duration_ms)
    .bind(num_turns)
    .bind(session_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Get a single session by ID.
pub async fn session_by_id(pool: &SqlitePool, session_id: &str) -> Result<Option<SessionRow>> {
    let sql = format!("{} WHERE session_id = ?", select_sessions_sql());
    let row: Option<SessionRow> = sqlx::query_as(&sql)
        .bind(session_id)
        .fetch_optional(pool)
        .await?;
    Ok(row)
}

/// List sessions with NULL cost (for startup cost reconciliation).
pub async fn list_sessions_missing_cost(pool: &SqlitePool) -> Result<Vec<SessionRow>> {
    let sql = format!(
        "{} WHERE cost_usd IS NULL AND status != 'running'",
        select_sessions_sql()
    );
    let rows: Vec<SessionRow> = sqlx::query_as(&sql).fetch_all(pool).await?;
    Ok(rows)
}

/// Get the credential_id for a session.
pub async fn get_credential_id(pool: &SqlitePool, session_id: &str) -> Result<Option<i64>> {
    let row: Option<(Option<i64>,)> =
        sqlx::query_as("SELECT credential_id FROM cc_sessions WHERE session_id = ?")
            .bind(session_id)
            .fetch_optional(pool)
            .await?;
    Ok(row.and_then(|r| r.0))
}

/// Check if a specific session is currently running (single-row query).
pub async fn is_session_running(pool: &SqlitePool, session_id: &str) -> Result<bool> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM cc_sessions WHERE session_id = ? AND status = 'running'",
    )
    .bind(session_id)
    .fetch_one(pool)
    .await?;
    Ok(count > 0)
}

/// List all sessions currently marked as running (for tick reconciliation).
pub async fn list_running_sessions(pool: &SqlitePool) -> Result<Vec<SessionRow>> {
    let sql = format!("{} WHERE status = 'running'", select_sessions_sql());
    let rows: Vec<SessionRow> = sqlx::query_as(&sql).fetch_all(pool).await?;
    Ok(rows)
}

/// List running sessions for a specific task (for cancel cleanup).
pub async fn list_running_sessions_for_task(
    pool: &SqlitePool,
    task_id: i64,
) -> Result<Vec<SessionRow>> {
    let sql = format!(
        "{} WHERE task_id = ? AND status = 'running'",
        select_sessions_sql()
    );
    let rows: Vec<SessionRow> = sqlx::query_as(&sql).bind(task_id).fetch_all(pool).await?;
    Ok(rows)
}

/// Find session_id for a running session by worker_name.
pub async fn find_session_id_by_worker_name(
    pool: &SqlitePool,
    worker_name: &str,
) -> Result<Option<String>> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT session_id FROM cc_sessions WHERE worker_name = ? AND status = 'running' LIMIT 1",
    )
    .bind(worker_name)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|(sid,)| sid))
}

#[cfg(test)]
mod tests {
    use super::*;
    use global_db::Db;

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
                task_id: Some(1),
                scout_item_id: None,
                worker_name: Some("main-v1"),
                resumed_at: None,
                credential_id: None,
            },
        )
        .await
        .unwrap();

        let (rows, total) = list_sessions(&pool, 1, 50, None, None).await.unwrap();
        assert_eq!(total, 1);
        assert_eq!(rows[0].session_id, "s1");
        assert_eq!(rows[0].cost_usd, Some(1.5));
        assert_eq!(rows[0].turn_count, 0);
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
                task_id: Some(1),
                scout_item_id: None,
                worker_name: None,
                resumed_at: None,
                credential_id: None,
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
                resumed_at: None,
                credential_id: None,
            },
        )
        .await
        .unwrap();

        let (rows, _) = list_sessions(&pool, 1, 50, None, None).await.unwrap();
        assert_eq!(rows[0].cost_usd, Some(1.5)); // 1.0 + 0.5
        assert_eq!(rows[0].duration_ms, Some(15000)); // 10000 + 5000
        assert_eq!(rows[0].turn_count, 1);
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
                resumed_at: None,
                credential_id: None,
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
                resumed_at: None,
                credential_id: None,
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
                task_id: Some(1),
                scout_item_id: None,
                worker_name: Some("w1"),
                resumed_at: None,
                credential_id: None,
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
                resumed_at: None,
                credential_id: None,
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
                task_id: Some(1),
                scout_item_id: None,
                worker_name: None,
                resumed_at: None,
                credential_id: None,
            },
        )
        .await
        .unwrap();

        let (rows, total) = list_sessions(&pool, 1, 50, Some("scout"), None)
            .await
            .unwrap();
        assert_eq!(total, 1);
        assert_eq!(rows[0].caller, "scout-process");
    }

    #[tokio::test]
    async fn filter_by_status() {
        let pool = test_pool().await;
        for (sid, status) in [
            ("s1", SessionStatus::Running),
            ("s2", SessionStatus::Stopped),
            ("s3", SessionStatus::Failed),
        ] {
            upsert_session(
                &pool,
                &SessionUpsert {
                    session_id: sid,
                    created_at: "2026-03-26T00:00:00Z",
                    caller: "worker",
                    cwd: "/tmp",
                    model: "opus",
                    status,
                    cost_usd: None,
                    duration_ms: None,
                    resumed: false,
                    task_id: None,
                    scout_item_id: None,
                    worker_name: None,
                    resumed_at: None,
                    credential_id: None,
                },
            )
            .await
            .unwrap();
        }

        let (rows, total) = list_sessions(&pool, 1, 50, None, Some("running"))
            .await
            .unwrap();
        assert_eq!(total, 1);
        assert_eq!(rows[0].session_id, "s1");

        let (rows, total) = list_sessions(&pool, 1, 50, None, Some("stopped"))
            .await
            .unwrap();
        assert_eq!(total, 1);
        assert_eq!(rows[0].session_id, "s2");

        let (rows, total) = list_sessions(&pool, 1, 50, None, Some("failed"))
            .await
            .unwrap();
        assert_eq!(total, 1);
        assert_eq!(rows[0].session_id, "s3");

        let (_, total) = list_sessions(&pool, 1, 50, None, None).await.unwrap();
        assert_eq!(total, 3);
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
                resumed_at: None,
                credential_id: None,
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
                resumed_at: None,
                credential_id: None,
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
                resumed_at: None,
                credential_id: None,
            },
        )
        .await
        .unwrap();

        let rows = list_sessions_for_scout_item(&pool, 42).await.unwrap();
        assert_eq!(rows.len(), 2);
    }
}
