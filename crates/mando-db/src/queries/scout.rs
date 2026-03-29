//! Scout item queries.

use anyhow::{bail, Result};
use mando_types::{ScoutItem, ScoutStatus};
use sqlx::SqlitePool;

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
    let now = mando_types::now_rfc3339();
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
    let row: Option<ItemRow> = sqlx::query_as(
        "SELECT id, url, type, title, status, relevance, quality,
                date_added, date_processed, added_by, error_count, source_name, date_published
         FROM scout_items WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| r.into_item()))
}

/// Get item by URL.
pub async fn get_item_by_url(pool: &SqlitePool, url: &str) -> Result<Option<ScoutItem>> {
    let row: Option<ItemRow> = sqlx::query_as(
        "SELECT id, url, type, title, status, relevance, quality,
                date_added, date_processed, added_by, error_count, source_name, date_published
         FROM scout_items WHERE url = ?",
    )
    .bind(url)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| r.into_item()))
}

/// List items, optionally filtered by status.
pub async fn list_items(pool: &SqlitePool, status: Option<&str>) -> Result<Vec<ScoutItem>> {
    let rows: Vec<ItemRow> = match status {
        Some("all") => {
            sqlx::query_as(
                "SELECT id, url, type, title, status, relevance, quality,
                        date_added, date_processed, added_by, error_count, source_name, date_published
                 FROM scout_items ORDER BY id",
            )
            .fetch_all(pool)
            .await?
        }
        Some(s) => {
            sqlx::query_as(
                "SELECT id, url, type, title, status, relevance, quality,
                        date_added, date_processed, added_by, error_count, source_name, date_published
                 FROM scout_items WHERE status = ? ORDER BY id",
            )
            .bind(s)
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query_as(
                "SELECT id, url, type, title, status, relevance, quality,
                        date_added, date_processed, added_by, error_count, source_name, date_published
                 FROM scout_items WHERE status NOT IN ('archived', 'saved', 'error') ORDER BY id",
            )
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows.into_iter().map(|r| r.into_item()).collect())
}

/// List processable items.
pub async fn list_processable(pool: &SqlitePool) -> Result<Vec<ScoutItem>> {
    let rows: Vec<ItemRow> = sqlx::query_as(
        "SELECT id, url, type, title, status, relevance, quality,
                date_added, date_processed, added_by, error_count, source_name, date_published
         FROM scout_items WHERE status = 'pending' ORDER BY id",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|r| r.into_item()).collect())
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
        "SELECT id, url, type, title, status, relevance, quality,
                date_added, date_processed, added_by, error_count, source_name, date_published
         FROM scout_items {where_clause} ORDER BY id DESC LIMIT ? OFFSET ?"
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
        rows.into_iter().map(|r| r.into_item()).collect(),
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

/// Update item status.
pub async fn update_status(pool: &SqlitePool, id: i64, status: &str) -> Result<()> {
    let result = sqlx::query("UPDATE scout_items SET status = ? WHERE id = ?")
        .bind(status)
        .bind(id)
        .execute(pool)
        .await?;
    if result.rows_affected() == 0 {
        bail!("item #{id} not found");
    }
    Ok(())
}

/// Conditional status update (TOCTOU safe).
pub async fn update_status_if(
    pool: &SqlitePool,
    id: i64,
    status: &str,
    only_from: &[&str],
) -> Result<bool> {
    if only_from.is_empty() {
        bail!("only_from must not be empty");
    }
    let placeholders: String = only_from.iter().map(|_| "?").collect::<Vec<_>>().join(",");
    let sql =
        format!("UPDATE scout_items SET status = ? WHERE id = ? AND status IN ({placeholders})");
    let mut q = sqlx::query(&sql).bind(status).bind(id);
    for s in only_from {
        q = q.bind(*s);
    }
    let result = q.execute(pool).await?;
    Ok(result.rows_affected() > 0)
}

/// Update processed results.
pub async fn update_processed(
    pool: &SqlitePool,
    id: i64,
    title: &str,
    relevance: i64,
    quality: i64,
    source_name: Option<&str>,
    date_published: Option<&str>,
) -> Result<bool> {
    let now = mando_types::now_rfc3339();
    let result = sqlx::query(
        "UPDATE scout_items
         SET title = ?, relevance = ?, quality = ?,
             source_name = ?, status = 'processed', date_processed = ?,
             date_published = ?
         WHERE id = ? AND status IN ('pending', 'fetched', 'error')",
    )
    .bind(title)
    .bind(relevance)
    .bind(quality)
    .bind(source_name)
    .bind(&now)
    .bind(date_published)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Restore pre-process metadata and force status back to pending.
pub async fn rollback_processed(pool: &SqlitePool, item: &ScoutItem) -> Result<()> {
    let result = sqlx::query(
        "UPDATE scout_items
         SET title = ?, relevance = ?, quality = ?,
             source_name = ?, status = 'pending', date_processed = ?,
             date_published = ?
         WHERE id = ?",
    )
    .bind(item.title.as_deref())
    .bind(item.relevance)
    .bind(item.quality)
    .bind(item.source_name.as_deref())
    .bind(item.date_processed.as_deref())
    .bind(item.date_published.as_deref())
    .bind(item.id)
    .execute(pool)
    .await?;
    if result.rows_affected() == 0 {
        bail!("item #{} not found", item.id);
    }
    Ok(())
}

/// Set title without changing status.
pub async fn set_title(pool: &SqlitePool, id: i64, title: &str) -> Result<()> {
    let result = sqlx::query("UPDATE scout_items SET title = ? WHERE id = ?")
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
pub async fn delete_item(pool: &SqlitePool, id: i64) -> Result<bool> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM cc_sessions WHERE scout_item_id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    let result = sqlx::query("DELETE FROM scout_items WHERE id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
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

/// Increment error count and set status to error.
pub async fn increment_error_count(pool: &SqlitePool, id: i64) -> Result<()> {
    sqlx::query(
        "UPDATE scout_items
         SET error_count = COALESCE(error_count, 0) + 1, status = 'error'
         WHERE id = ?",
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

// ── Internal ────────────────────────────────────────────────────────────────

#[derive(sqlx::FromRow)]
struct ItemRow {
    id: i64,
    url: Option<String>,
    r#type: Option<String>,
    title: Option<String>,
    status: Option<String>,
    relevance: Option<i64>,
    quality: Option<i64>,
    date_added: Option<String>,
    date_processed: Option<String>,
    added_by: Option<String>,
    error_count: Option<i64>,
    source_name: Option<String>,
    date_published: Option<String>,
}

impl ItemRow {
    fn into_item(self) -> ScoutItem {
        let status_str = self.status.unwrap_or_else(|| "pending".into());
        let status = parse_status(&status_str);
        ScoutItem {
            id: self.id,
            url: self.url.unwrap_or_default(),
            item_type: self.r#type.unwrap_or_else(|| "other".into()),
            title: self.title,
            status,
            relevance: self.relevance,
            quality: self.quality,
            date_added: self.date_added.unwrap_or_default(),
            date_processed: self.date_processed,
            added_by: self.added_by,
            error_count: self.error_count.unwrap_or(0),
            source_name: self.source_name,
            date_published: self.date_published,
        }
    }
}

fn parse_status(s: &str) -> ScoutStatus {
    s.parse().unwrap_or_else(|_| {
        tracing::warn!(status = s, "unknown scout status, defaulting to Pending");
        ScoutStatus::Pending
    })
}
