//! Async wrapper around mando-db for scout items.

use std::collections::HashMap;

use crate::{ScoutItem, ScoutStatus};
use anyhow::Result;
use sqlx::SqlitePool;

use crate::io::queries::scout as dq;
use crate::service::fuzzy::fuzzy_score;
use sessions::queries as sq;

pub use dq::{ListQuery, ListResult};
pub use sq::SessionRow;

/// Database handle for scout operations.
pub struct ScoutDb {
    pool: SqlitePool,
}

impl ScoutDb {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub async fn add_item(
        &self,
        url: &str,
        item_type: &str,
        added_by: Option<&str>,
    ) -> Result<(ScoutItem, bool)> {
        dq::add_item(&self.pool, url, item_type, added_by).await
    }

    pub async fn get_item(&self, id: i64) -> Result<Option<ScoutItem>> {
        dq::get_item(&self.pool, id).await
    }

    pub async fn list_processable(&self) -> Result<Vec<ScoutItem>> {
        dq::list_processable(&self.pool).await
    }

    pub async fn update_status(&self, id: i64, status: &str) -> Result<()> {
        dq::update_status(&self.pool, id, status).await
    }

    pub async fn update_status_if(
        &self,
        id: i64,
        status: &str,
        only_from: &[&str],
    ) -> Result<bool> {
        dq::update_status_if(&self.pool, id, status, only_from).await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn update_processed(
        &self,
        id: i64,
        title: &str,
        relevance: i64,
        quality: i64,
        source_name: Option<&str>,
        date_published: Option<&str>,
        summary: &str,
        article: &str,
    ) -> Result<bool> {
        dq::update_processed(
            &self.pool,
            id,
            title,
            relevance,
            quality,
            source_name,
            date_published,
            summary,
            article,
        )
        .await
    }

    pub async fn get_summary(&self, id: i64) -> Result<Option<String>> {
        dq::get_summary(&self.pool, id).await
    }

    pub async fn get_article(&self, id: i64) -> Result<Option<String>> {
        dq::get_article(&self.pool, id).await
    }

    pub async fn set_article(&self, id: i64, article: &str) -> Result<()> {
        dq::set_article(&self.pool, id, article).await
    }

    pub async fn set_title(&self, id: i64, title: &str) -> Result<()> {
        dq::set_title(&self.pool, id, title).await
    }

    pub async fn delete_item(&self, id: i64) -> Result<bool> {
        dq::delete_item(&self.pool, id).await
    }

    pub async fn increment_error_count(&self, id: i64) -> Result<()> {
        dq::increment_error_count(&self.pool, id).await
    }

    pub async fn increment_error_count_if_status(
        &self,
        id: i64,
        allowed_statuses: &[ScoutStatus],
    ) -> Result<bool> {
        dq::increment_error_count_if_status(&self.pool, id, allowed_statuses).await
    }

    /// Record a CC session via the unified sessions table.
    ///
    /// Pass `Some(id)` for item-level operations (process, article, act, qa)
    /// or `None` for topic-level operations (research).
    pub async fn record_session(
        &self,
        item_id: Option<i64>,
        session_id: &str,
        caller: &str,
        cost_usd: Option<f64>,
        duration_ms: Option<u64>,
        credential_id: Option<i64>,
    ) -> Result<()> {
        let now = global_types::now_rfc3339();
        sq::upsert_session(
            &self.pool,
            &sq::SessionUpsert {
                provider: global_types::TaskProvider::Claude,
                session_id,
                created_at: &now,
                caller,
                cwd: "",
                model: "",
                status: global_types::SessionStatus::Stopped,
                cost_usd,
                duration_ms: duration_ms.map(|d| d as i64),
                resumed: false,
                task_id: None,
                scout_item_id: item_id,
                worker_name: None,
                resumed_at: None,
                credential_id,
                error: None,
                api_error_status: None,
            },
        )
        .await
    }

    /// List all sessions for a scout item, newest first.
    pub async fn list_sessions_for_item(&self, item_id: i64) -> Result<Vec<SessionRow>> {
        sq::list_sessions_for_scout_item(&self.pool, item_id).await
    }

    /// Count items grouped by status, filtered by search/type.
    /// Uses fuzzy matching when search is set.
    pub async fn count_by_status(&self, q: &ListQuery) -> Result<HashMap<String, usize>> {
        let has_search = q.search.as_ref().is_some_and(|s| !s.is_empty());

        if !has_search {
            return dq::count_by_status(&self.pool, q.item_type.as_deref()).await;
        }

        // Fuzzy path: fetch all items matching type (ignoring status), score, count by status.
        let all_items = dq::list_items(&self.pool, Some("all")).await?;
        let query = q.search.as_deref().unwrap_or("");
        let mut counts = HashMap::new();
        for item in all_items {
            if q.item_type.as_ref().is_some_and(|t| t != &item.item_type) {
                continue;
            }
            let title_score = fuzzy_score(query, item.title.as_deref().unwrap_or(""));
            let url_score = fuzzy_score(query, &item.url);
            if title_score.max(url_score) > 0.0 {
                *counts.entry(item.status.as_str().to_string()).or_insert(0) += 1;
            }
        }
        Ok(counts)
    }

    /// Query items with search, type filter, status filter, and pagination.
    /// Uses fuzzy matching when search is set.
    pub async fn query_items(&self, q: &ListQuery) -> Result<ListResult> {
        let has_search = q.search.as_ref().is_some_and(|s| !s.is_empty());
        if has_search {
            return self.fuzzy_query(q).await;
        }
        self.sql_query(q).await
    }

    async fn sql_query(&self, q: &ListQuery) -> Result<ListResult> {
        let (items, total) = dq::query_items_paginated(
            &self.pool,
            q.status.as_deref(),
            q.item_type.as_deref(),
            q.page,
            q.per_page,
        )
        .await?;
        Ok(ListResult { items, total })
    }

    /// Upper bound on rows loaded into memory for a single fuzzy query.
    ///
    /// Scout items are typically in the low thousands; this cap is a safety
    /// valve against unbounded memory use on very large collections. When
    /// hit, we emit a `warn` so operators can migrate to FTS5 before the
    /// ceiling actually bites user-visible results.
    const FUZZY_SCAN_CAP: usize = 5000;

    async fn fuzzy_query(&self, q: &ListQuery) -> Result<ListResult> {
        let all = self
            .fetch_filtered(q.status.as_deref(), q.item_type.as_deref())
            .await?;
        let loaded = all.len();
        if loaded >= Self::FUZZY_SCAN_CAP {
            tracing::warn!(
                module = "scout-db",
                loaded,
                cap = Self::FUZZY_SCAN_CAP,
                "scout fuzzy search loaded a large row set — consider FTS5 migration"
            );
        }
        let query = q.search.as_deref().unwrap_or("");

        let mut scored: Vec<(ScoutItem, f64)> = Vec::with_capacity(loaded.min(256));
        for item in all.into_iter().take(Self::FUZZY_SCAN_CAP) {
            let title_score = fuzzy_score(query, item.title.as_deref().unwrap_or(""));
            let url_score = fuzzy_score(query, &item.url);
            let best = title_score.max(url_score);
            if best > 0.0 {
                scored.push((item, best));
            }
        }

        // total_cmp is NaN-safe; fuzzy_score cannot currently produce NaN but
        // this future-proofs against any scoring change that introduces one.
        scored.sort_by(|a, b| b.1.total_cmp(&a.1));

        let total = scored.len();
        let per_page = if q.per_page == 0 { 50 } else { q.per_page };
        let offset = q.page * per_page;
        let items: Vec<ScoutItem> = scored
            .into_iter()
            .skip(offset)
            .take(per_page)
            .map(|(item, _)| item)
            .collect();

        Ok(ListResult { items, total })
    }

    async fn fetch_filtered(
        &self,
        status: Option<&str>,
        item_type: Option<&str>,
    ) -> Result<Vec<ScoutItem>> {
        let items = dq::list_items(&self.pool, status).await?;
        if let Some(t) = item_type {
            Ok(items.into_iter().filter(|i| i.item_type == t).collect())
        } else {
            Ok(items)
        }
    }
}
