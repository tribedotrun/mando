//! Scout item queries.

use crate::{ScoutItem, ScoutStatus};
use anyhow::{bail, Result};
use sqlx::SqlitePool;

mod lifecycle;

pub use lifecycle::{
    increment_error_count, increment_error_count_if_status, reset_error_state, reset_stale_fetched,
    update_processed, update_status, update_status_if,
};

/// Column list for ItemRow queries - single source of truth.
const SELECT_COLS: &str = "\
    id, url, type, title, status, relevance, quality, \
    date_added, date_processed, added_by, error_count, source_name, date_published, rev, \
    research_run_id";

fn select_items_sql() -> &'static str {
    static SQL: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    SQL.get_or_init(|| format!("SELECT {SELECT_COLS} FROM scout_items"))
}

/// Query parameters for listing scout items.
#[derive(Debug, Default)]
pub struct ListQuery {
    pub search: Option<String>,
    pub status: Option<String>,
    pub item_type: Option<String>,
    pub page: usize,
    pub per_page: usize,
}

/// Paginated result.
#[derive(Debug)]
pub struct ListResult {
    pub items: Vec<ScoutItem>,
    pub total: usize,
}

/// Add a new item or return existing.
pub async fn add_item(
    pool: &SqlitePool,
    url: &str,
    item_type: &str,
    added_by: Option<&str>,
) -> Result<(ScoutItem, bool)> {
    let now = global_types::now_rfc3339();
    let result = sqlx::query(
        "INSERT OR IGNORE INTO scout_items (url, type, status, date_added, added_by)
         VALUES (?, ?, 'pending', ?, ?)",
    )
    .bind(url)
    .bind(item_type)
    .bind(&now)
    .bind(added_by)
    .execute(pool)
    .await?;
    let was_inserted = result.rows_affected() > 0;
    let item = get_item_by_url(pool, url)
        .await?
        .ok_or_else(|| anyhow::anyhow!("item not found after insert"))?;
    Ok((item, was_inserted))
}

/// Get item by ID.
pub async fn get_item(pool: &SqlitePool, id: i64) -> Result<Option<ScoutItem>> {
    let sql = format!("{} WHERE id = ?", select_items_sql());
    let row: Option<ItemRow> = sqlx::query_as(&sql).bind(id).fetch_optional(pool).await?;
    row.map(ItemRow::into_item).transpose()
}

/// Get item by URL.
pub async fn get_item_by_url(pool: &SqlitePool, url: &str) -> Result<Option<ScoutItem>> {
    let sql = format!("{} WHERE url = ?", select_items_sql());
    let row: Option<ItemRow> = sqlx::query_as(&sql).bind(url).fetch_optional(pool).await?;
    row.map(ItemRow::into_item).transpose()
}

/// List items, optionally filtered by status.
pub async fn list_items(pool: &SqlitePool, status: Option<&str>) -> Result<Vec<ScoutItem>> {
    let base = select_items_sql();
    let (sql, bind_status) = match status {
        Some("all") => (format!("{base} ORDER BY id"), None),
        Some(s) => (format!("{base} WHERE status = ? ORDER BY id"), Some(s)),
        None => (
            format!("{base} WHERE status NOT IN ('archived', 'saved', 'error') ORDER BY id"),
            None,
        ),
    };
    let mut q = sqlx::query_as::<_, ItemRow>(&sql);
    if let Some(s) = bind_status {
        q = q.bind(s);
    }
    let rows = q.fetch_all(pool).await?;
    rows.into_iter().map(ItemRow::into_item).collect()
}

/// List processable items.
pub async fn list_processable(pool: &SqlitePool) -> Result<Vec<ScoutItem>> {
    let sql = format!(
        "{} WHERE status = 'pending' ORDER BY id",
        select_items_sql()
    );
    let rows: Vec<ItemRow> = sqlx::query_as(&sql).fetch_all(pool).await?;
    rows.into_iter().map(ItemRow::into_item).collect()
}

/// Paginated query with status/type filters and SQL-level LIMIT/OFFSET.
pub async fn query_items_paginated(
    pool: &SqlitePool,
    status: Option<&str>,
    item_type: Option<&str>,
    page: usize,
    per_page: usize,
) -> Result<(Vec<ScoutItem>, usize)> {
    let per_page = if per_page == 0 { 50 } else { per_page };
    let offset = page * per_page;

    // Build WHERE clause dynamically based on filters.
    let mut conditions: Vec<String> = Vec::new();
    match status {
        Some("all") => {}
        Some(_) => conditions.push("status = ?".into()),
        None => conditions.push("status NOT IN ('archived', 'saved', 'error')".into()),
    }
    if item_type.is_some() {
        conditions.push("type = ?".into());
    }
    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    };

    // Count query.
    let count_sql = format!("SELECT COUNT(*) FROM scout_items {where_clause}");
    let mut q = sqlx::query_scalar::<_, i64>(&count_sql);
    if let Some(s) = status {
        if s != "all" {
            q = q.bind(s);
        }
    }
    if let Some(t) = item_type {
        q = q.bind(t);
    }
    let total: i64 = q.fetch_one(pool).await?;

    // Data query with LIMIT/OFFSET.
    let select_sql = format!(
        "{} {where_clause} ORDER BY id DESC LIMIT ? OFFSET ?",
        select_items_sql()
    );
    let mut q = sqlx::query_as::<_, ItemRow>(&select_sql);
    if let Some(s) = status {
        if s != "all" {
            q = q.bind(s);
        }
    }
    if let Some(t) = item_type {
        q = q.bind(t);
    }
    q = q.bind(per_page as i64).bind(offset as i64);
    let rows = q.fetch_all(pool).await?;

    Ok((
        rows.into_iter()
            .map(ItemRow::into_item)
            .collect::<Result<Vec<_>>>()?,
        total as usize,
    ))
}

/// Count items by status.
pub async fn count_by_status(
    pool: &SqlitePool,
    item_type: Option<&str>,
) -> Result<std::collections::HashMap<String, usize>> {
    let rows: Vec<(String, i64)> = if let Some(t) = item_type {
        sqlx::query_as("SELECT status, COUNT(*) FROM scout_items WHERE type = ? GROUP BY status")
            .bind(t)
            .fetch_all(pool)
            .await?
    } else {
        sqlx::query_as("SELECT status, COUNT(*) FROM scout_items GROUP BY status")
            .fetch_all(pool)
            .await?
    };
    Ok(rows.into_iter().map(|(s, c)| (s, c as usize)).collect())
}

/// Fetch the summary text for a scout item.
pub async fn get_summary(pool: &SqlitePool, id: i64) -> Result<Option<String>> {
    let row: Option<(Option<String>,)> =
        sqlx::query_as("SELECT summary FROM scout_items WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await?;
    Ok(row.and_then(|(s,)| s))
}

/// Fetch the article markdown for a scout item.
pub async fn get_article(pool: &SqlitePool, id: i64) -> Result<Option<String>> {
    let row: Option<(Option<String>,)> =
        sqlx::query_as("SELECT article FROM scout_items WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await?;
    Ok(row.and_then(|(s,)| s))
}

/// Replace the article markdown for a scout item.
pub async fn set_article(pool: &SqlitePool, id: i64, article: &str) -> Result<()> {
    let result = sqlx::query("UPDATE scout_items SET article = ?, rev = rev + 1 WHERE id = ?")
        .bind(article)
        .bind(id)
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        bail!("item #{id} not found");
    }
    Ok(())
}

/// Set title without changing status.
pub async fn set_title(pool: &SqlitePool, id: i64, title: &str) -> Result<()> {
    let result = sqlx::query("UPDATE scout_items SET title = ?, rev = rev + 1 WHERE id = ?")
        .bind(title)
        .bind(id)
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        bail!("item #{id} not found");
    }
    Ok(())
}

/// Delete an item and its linked sessions atomically.
///
/// If the item belonged to a research run, decrement that run's `added_count`
/// in the same transaction so the run's discovered-items total stays in sync.
pub async fn delete_item(pool: &SqlitePool, id: i64) -> Result<bool> {
    let mut tx = pool.begin().await?;
    let research_run_id: Option<i64> =
        sqlx::query_scalar("SELECT research_run_id FROM scout_items WHERE id = ?")
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
            .flatten();
    sqlx::query("DELETE FROM cc_sessions WHERE scout_item_id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    let result = sqlx::query("DELETE FROM scout_items WHERE id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    if result.rows_affected() > 0 {
        if let Some(run_id) = research_run_id {
            sqlx::query(
                "UPDATE scout_research_runs
                 SET added_count = MAX(added_count - 1, 0), rev = rev + 1
                 WHERE id = ?",
            )
            .bind(run_id)
            .execute(&mut *tx)
            .await?;
        }
    }
    tx.commit().await?;
    Ok(result.rows_affected() > 0)
}

/// Bulk-fetch scout item titles by IDs.
pub async fn item_titles(
    pool: &SqlitePool,
    ids: &[i64],
) -> Result<std::collections::HashMap<i64, String>> {
    if ids.is_empty() {
        return Ok(std::collections::HashMap::new());
    }
    let placeholders: String = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT id, title FROM scout_items WHERE id IN ({placeholders}) AND title IS NOT NULL"
    );
    let mut query = sqlx::query_as::<_, (i64, String)>(&sql);
    for id in ids {
        query = query.bind(id);
    }
    let rows = query.fetch_all(pool).await?;
    Ok(rows.into_iter().collect())
}

// ── Internal ────────────────────────────────────────────────────────────────

#[derive(sqlx::FromRow)]
struct ItemRow {
    id: i64,
    url: String,
    r#type: String,
    title: Option<String>,
    status: Option<String>,
    relevance: Option<i64>,
    quality: Option<i64>,
    date_added: String,
    date_processed: Option<String>,
    added_by: Option<String>,
    error_count: Option<i64>,
    source_name: Option<String>,
    date_published: Option<String>,
    rev: i64,
    research_run_id: Option<i64>,
}

impl ItemRow {
    fn into_item(self) -> Result<ScoutItem> {
        let status_str = self.status.unwrap_or_else(|| "pending".into());
        let status = parse_status(&status_str)?;
        Ok(ScoutItem {
            id: self.id,
            url: self.url,
            item_type: self.r#type,
            title: self.title,
            status,
            relevance: self.relevance,
            quality: self.quality,
            date_added: self.date_added,
            date_processed: self.date_processed,
            added_by: self.added_by,
            error_count: self.error_count.unwrap_or(0),
            source_name: self.source_name,
            date_published: self.date_published,
            rev: self.rev,
            research_run_id: self.research_run_id,
        })
    }
}

/// List items belonging to a specific research run.
pub async fn list_items_by_run(pool: &SqlitePool, run_id: i64) -> Result<Vec<ScoutItem>> {
    let sql = format!(
        "{} WHERE research_run_id = ? ORDER BY id",
        select_items_sql()
    );
    let rows: Vec<ItemRow> = sqlx::query_as(&sql).bind(run_id).fetch_all(pool).await?;
    rows.into_iter().map(ItemRow::into_item).collect()
}

/// Set the research_run_id FK on a scout item.
pub async fn set_research_run_id(pool: &SqlitePool, id: i64, run_id: i64) -> Result<()> {
    sqlx::query("UPDATE scout_items SET research_run_id = ?, rev = rev + 1 WHERE id = ?")
        .bind(run_id)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

fn parse_status(s: &str) -> Result<ScoutStatus> {
    s.parse()
        .map_err(|err: String| anyhow::anyhow!("invalid scout status in database: {err}"))
}
