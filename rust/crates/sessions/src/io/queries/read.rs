use std::collections::HashMap;

use anyhow::Result;
use sqlx::SqlitePool;

use crate::io::queries::SessionRow;
use crate::SessionCaller;

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

pub async fn list_sessions(
    pool: &SqlitePool,
    page: usize,
    per_page: usize,
    group: Option<&str>,
    status: Option<&str>,
) -> Result<(Vec<SessionRow>, usize)> {
    let per_page = if per_page == 0 { 50 } else { per_page };
    let offset = page.saturating_sub(1) * per_page;
    let mut where_conditions: Vec<String> = Vec::new();
    let mut params: Vec<String> = Vec::new();

    if let Some(g) = group {
        let group_callers: Vec<&SessionCaller> = SessionCaller::all()
            .iter()
            .filter(|c| c.group().as_str() == g)
            .collect();

        let mut caller_conditions: Vec<String> = Vec::new();
        let exact: Vec<&str> = group_callers.iter().map(|c| c.as_str()).collect();
        if !exact.is_empty() {
            let ph: String = exact.iter().map(|_| "?").collect::<Vec<_>>().join(",");
            caller_conditions.push(format!("caller IN ({ph})"));
            params.extend(exact.iter().map(|s| (*s).to_string()));
        }

        for c in &group_callers {
            if let Some(prefix) = c.like_prefix() {
                caller_conditions.push("caller LIKE ?".to_string());
                params.push(prefix.to_string());
            }
        }

        if caller_conditions.is_empty() {
            return Ok((Vec::new(), 0));
        }
        where_conditions.push(format!("({})", caller_conditions.join(" OR ")));
    }

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
            super::select_sessions_sql()
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
            super::select_sessions_sql()
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

pub async fn list_sessions_for_task(pool: &SqlitePool, task_id: i64) -> Result<Vec<SessionRow>> {
    let sql = format!(
        "{} WHERE task_id = ? ORDER BY created_at DESC",
        super::select_sessions_sql()
    );
    let rows: Vec<SessionRow> = sqlx::query_as(&sql).bind(task_id).fetch_all(pool).await?;
    Ok(rows)
}

pub async fn list_sessions_for_scout_item(
    pool: &SqlitePool,
    item_id: i64,
) -> Result<Vec<SessionRow>> {
    let sql = format!(
        "{} WHERE scout_item_id = ? ORDER BY created_at DESC",
        super::select_sessions_sql()
    );
    let rows: Vec<SessionRow> = sqlx::query_as(&sql).bind(item_id).fetch_all(pool).await?;
    Ok(rows)
}

pub async fn delete_sessions_for_task(pool: &SqlitePool, task_id: i64) -> Result<u64> {
    let result = sqlx::query("DELETE FROM cc_sessions WHERE task_id = ?")
        .bind(task_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected())
}

pub async fn session_cwd(pool: &SqlitePool, session_id: &str) -> Result<Option<String>> {
    let row: Option<(String,)> = sqlx::query_as("SELECT cwd FROM cc_sessions WHERE session_id = ?")
        .bind(session_id)
        .fetch_optional(pool)
        .await?;
    Ok(row.and_then(|(cwd,)| if cwd.is_empty() { None } else { Some(cwd) }))
}

pub async fn total_session_cost(pool: &SqlitePool) -> Result<f64> {
    let cost: f64 = sqlx::query_scalar("SELECT COALESCE(SUM(cost_usd), 0.0) FROM cc_sessions")
        .fetch_one(pool)
        .await?;
    Ok(cost)
}

pub async fn session_by_id(pool: &SqlitePool, session_id: &str) -> Result<Option<SessionRow>> {
    let sql = format!("{} WHERE session_id = ?", super::select_sessions_sql());
    let row: Option<SessionRow> = sqlx::query_as(&sql)
        .bind(session_id)
        .fetch_optional(pool)
        .await?;
    Ok(row)
}

pub async fn list_sessions_missing_cost(pool: &SqlitePool) -> Result<Vec<SessionRow>> {
    let sql = format!(
        "{} WHERE cost_usd IS NULL AND status != 'running'",
        super::select_sessions_sql()
    );
    let rows: Vec<SessionRow> = sqlx::query_as(&sql).fetch_all(pool).await?;
    Ok(rows)
}

pub async fn get_credential_id(pool: &SqlitePool, session_id: &str) -> Result<Option<i64>> {
    let row: Option<(Option<i64>,)> =
        sqlx::query_as("SELECT credential_id FROM cc_sessions WHERE session_id = ?")
            .bind(session_id)
            .fetch_optional(pool)
            .await?;
    Ok(row.and_then(|r| r.0))
}

pub async fn is_session_running(pool: &SqlitePool, session_id: &str) -> Result<bool> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM cc_sessions WHERE session_id = ? AND status = 'running'",
    )
    .bind(session_id)
    .fetch_one(pool)
    .await?;
    Ok(count > 0)
}

pub async fn list_running_sessions(pool: &SqlitePool) -> Result<Vec<SessionRow>> {
    let sql = format!("{} WHERE status = 'running'", super::select_sessions_sql());
    let rows: Vec<SessionRow> = sqlx::query_as(&sql).fetch_all(pool).await?;
    Ok(rows)
}

pub async fn list_running_sessions_for_task(
    pool: &SqlitePool,
    task_id: i64,
) -> Result<Vec<SessionRow>> {
    let sql = format!(
        "{} WHERE task_id = ? AND status = 'running'",
        super::select_sessions_sql()
    );
    let rows: Vec<SessionRow> = sqlx::query_as(&sql).bind(task_id).fetch_all(pool).await?;
    Ok(rows)
}

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
