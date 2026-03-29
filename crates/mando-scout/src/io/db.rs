//! Async wrapper around mando-db for scout items.

use std::collections::HashMap;

use anyhow::Result;
use mando_types::ScoutItem;
use sqlx::SqlitePool;

use crate::biz::fuzzy::fuzzy_score;
use mando_db::queries::{scout as dq, sessions as sq};

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

    pub async fn get_item_by_url(&self, url: &str) -> Result<Option<ScoutItem>> {
        dq::get_item_by_url(&self.pool, url).await
    }

    pub async fn list_items(&self, status: Option<&str>) -> Result<Vec<ScoutItem>> {
        dq::list_items(&self.pool, status).await
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

    pub async fn update_processed(
        &self,
        id: i64,
        title: &str,
        relevance: i64,
        quality: i64,
        source_name: Option<&str>,
        date_published: Option<&str>,
    ) -> Result<bool> {
        dq::update_processed(
            &self.pool,
            id,
            title,
            relevance,
            quality,
            source_name,
            date_published,
        )
        .await
    }

    /// Restore pre-process metadata after a downstream write failure.
    ///
    /// The item is made retryable again by forcing status back to `pending`.
    pub async fn rollback_processed(&self, item: &ScoutItem) -> Result<()> {
        dq::rollback_processed(&self.pool, item).await
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

    /// Record a CC session linked to a scout item via the unified sessions table.
    pub async fn record_session(
        &self,
        item_id: i64,
        session_id: &str,
        caller: &str,
        cost_usd: Option<f64>,
        duration_ms: Option<u64>,
    ) -> Result<()> {
        let now = mando_types::now_rfc3339();
        sq::upsert_session(
            &self.pool,
            &sq::SessionUpsert {
                session_id,
                created_at: &now,
                caller,
                cwd: "",
                model: "",
                status: mando_types::SessionStatus::Stopped,
                cost_usd,
                duration_ms: duration_ms.map(|d| d as i64),
                resumed: false,
                task_id: None,
                scout_item_id: Some(item_id),
                worker_name: None,
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

    async fn fuzzy_query(&self, q: &ListQuery) -> Result<ListResult> {
        let all = self
            .fetch_filtered(q.status.as_deref(), q.item_type.as_deref())
            .await?;
        let query = q.search.as_deref().unwrap_or("");

        let mut scored: Vec<(ScoutItem, f64)> = Vec::new();
        for item in all {
            let title_score = fuzzy_score(query, item.title.as_deref().unwrap_or(""));
            let url_score = fuzzy_score(query, &item.url);
            let best = title_score.max(url_score);
            if best > 0.0 {
                scored.push((item, best));
            }
        }

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

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

#[cfg(test)]
mod tests {
    use super::*;
    use mando_db::Db;
    use mando_types::ScoutStatus;

    async fn test_db() -> ScoutDb {
        let db = Db::open_in_memory().await.expect("open in-memory DB");
        ScoutDb::new(db.pool().clone())
    }

    #[tokio::test]
    async fn open_and_create_tables() {
        let db = test_db().await;
        let items = db.list_items(Some("all")).await.unwrap();
        assert!(items.is_empty());
    }

    #[tokio::test]
    async fn add_and_get_item() {
        let db = test_db().await;
        let (item, was_new) = db
            .add_item("https://example.com/post", "other", Some("test"))
            .await
            .unwrap();
        assert!(was_new);
        assert_eq!(item.url, "https://example.com/post");
        assert_eq!(item.item_type, "other");
        assert_eq!(item.added_by.as_deref(), Some("test"));
        assert_eq!(item.status, ScoutStatus::Pending);
        assert_eq!(item.error_count, 0);

        let fetched = db
            .get_item(item.id)
            .await
            .unwrap()
            .expect("item should exist");
        assert_eq!(fetched.url, item.url);
        assert_eq!(fetched.id, item.id);
    }

    #[tokio::test]
    async fn add_duplicate_url_returns_existing() {
        let db = test_db().await;
        let (first, first_new) = db
            .add_item("https://example.com/dup", "other", None)
            .await
            .unwrap();
        assert!(first_new);
        let (second, second_new) = db
            .add_item("https://example.com/dup", "other", None)
            .await
            .unwrap();
        assert!(!second_new);
        assert_eq!(first.id, second.id);
    }

    #[tokio::test]
    async fn list_items_default_excludes() {
        let db = test_db().await;
        db.add_item("https://a.com", "other", None).await.unwrap();
        db.add_item("https://b.com", "other", None).await.unwrap();

        let items = db.list_items(None).await.unwrap();
        assert_eq!(items.len(), 2);

        db.update_status(items[0].id, "archived").await.unwrap();
        let items = db.list_items(None).await.unwrap();
        assert_eq!(items.len(), 1);
    }

    #[tokio::test]
    async fn list_items_with_status_filter() {
        let db = test_db().await;
        db.add_item("https://a.com", "other", None).await.unwrap();
        let (item_b, _) = db.add_item("https://b.com", "other", None).await.unwrap();
        db.update_status(item_b.id, "processed").await.unwrap();

        let pending = db.list_items(Some("pending")).await.unwrap();
        assert_eq!(pending.len(), 1);
        let processed = db.list_items(Some("processed")).await.unwrap();
        assert_eq!(processed.len(), 1);
        let all = db.list_items(Some("all")).await.unwrap();
        assert_eq!(all.len(), 2);
    }

    #[tokio::test]
    async fn update_status() {
        let db = test_db().await;
        let (item, _) = db
            .add_item("https://x.com/post", "other", None)
            .await
            .unwrap();
        db.update_status(item.id, "fetched").await.unwrap();
        let fetched = db.get_item(item.id).await.unwrap().unwrap();
        assert_eq!(fetched.status, ScoutStatus::Fetched);
    }

    #[tokio::test]
    async fn update_processed() {
        let db = test_db().await;
        let (item, _) = db
            .add_item("https://x.com/post", "other", None)
            .await
            .unwrap();
        let changed = db
            .update_processed(
                item.id,
                "Great Article",
                90,
                85,
                Some("Blog"),
                Some("2026-01-15"),
            )
            .await
            .unwrap();
        assert!(changed);

        let updated = db.get_item(item.id).await.unwrap().unwrap();
        assert_eq!(updated.title.as_deref(), Some("Great Article"));
        assert_eq!(updated.relevance, Some(90));
        assert_eq!(updated.quality, Some(85));
        assert_eq!(updated.source_name.as_deref(), Some("Blog"));
        assert_eq!(updated.status, ScoutStatus::Processed);
        assert!(updated.date_processed.is_some());
        assert_eq!(updated.date_published.as_deref(), Some("2026-01-15"));
    }

    #[tokio::test]
    async fn delete_item() {
        let db = test_db().await;
        let (item, _) = db.add_item("https://del.com", "other", None).await.unwrap();
        assert!(db.delete_item(item.id).await.unwrap());
        assert!(db.get_item(item.id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn delete_item_also_removes_sessions() {
        let db = test_db().await;
        let (item, _) = db
            .add_item("https://del-sess.com", "other", None)
            .await
            .unwrap();
        db.record_session(item.id, "ses-1", "test", Some(0.5), Some(1000))
            .await
            .unwrap();
        db.record_session(item.id, "ses-2", "test", None, None)
            .await
            .unwrap();

        let sessions = db.list_sessions_for_item(item.id).await.unwrap();
        assert_eq!(sessions.len(), 2);

        assert!(db.delete_item(item.id).await.unwrap());

        let sessions = db.list_sessions_for_item(item.id).await.unwrap();
        assert!(sessions.is_empty());
    }

    #[tokio::test]
    async fn delete_nonexistent() {
        let db = test_db().await;
        assert!(!db.delete_item(999).await.unwrap());
    }

    #[tokio::test]
    async fn increment_error_count() {
        let db = test_db().await;
        let (item, _) = db.add_item("https://err.com", "other", None).await.unwrap();
        assert_eq!(item.error_count, 0);

        db.increment_error_count(item.id).await.unwrap();
        let updated = db.get_item(item.id).await.unwrap().unwrap();
        assert_eq!(updated.error_count, 1);
        assert_eq!(updated.status, ScoutStatus::Error);

        db.increment_error_count(item.id).await.unwrap();
        let updated2 = db.get_item(item.id).await.unwrap().unwrap();
        assert_eq!(updated2.error_count, 2);
    }

    #[tokio::test]
    async fn get_nonexistent() {
        let db = test_db().await;
        assert!(db.get_item(999).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn update_status_nonexistent_fails() {
        let db = test_db().await;
        let result = db.update_status(999, "processed").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn update_status_if_succeeds_from_valid_state() {
        let db = test_db().await;
        let (item, _) = db
            .add_item("https://cond.com", "other", None)
            .await
            .unwrap();
        assert_eq!(item.status, ScoutStatus::Pending);

        let updated = db
            .update_status_if(item.id, "fetched", &["pending", "error"])
            .await
            .unwrap();
        assert!(updated);
        let fetched = db.get_item(item.id).await.unwrap().unwrap();
        assert_eq!(fetched.status, ScoutStatus::Fetched);
    }

    #[tokio::test]
    async fn update_status_if_skips_wrong_state() {
        let db = test_db().await;
        let (item, _) = db
            .add_item("https://cond2.com", "other", None)
            .await
            .unwrap();
        db.update_processed(item.id, "T", 80, 80, None, None)
            .await
            .unwrap();

        let updated = db
            .update_status_if(item.id, "fetched", &["pending", "error"])
            .await
            .unwrap();
        assert!(!updated);
        let still_processed = db.get_item(item.id).await.unwrap().unwrap();
        assert_eq!(still_processed.status, ScoutStatus::Processed);
    }

    #[tokio::test]
    async fn update_processed_skips_already_processed() {
        let db = test_db().await;
        let (item, _) = db
            .add_item("https://toctou.com", "other", None)
            .await
            .unwrap();
        let changed = db
            .update_processed(item.id, "First", 90, 85, None, None)
            .await
            .unwrap();
        assert!(changed);
        let first = db.get_item(item.id).await.unwrap().unwrap();
        assert_eq!(first.title.as_deref(), Some("First"));

        let changed2 = db
            .update_processed(item.id, "Second", 50, 50, None, None)
            .await
            .unwrap();
        assert!(!changed2);
        let still_first = db.get_item(item.id).await.unwrap().unwrap();
        assert_eq!(still_first.title.as_deref(), Some("First"));
    }

    #[tokio::test]
    async fn update_processed_from_error_state() {
        let db = test_db().await;
        let (item, _) = db
            .add_item("https://retry.com", "other", None)
            .await
            .unwrap();
        db.increment_error_count(item.id).await.unwrap();
        let errored = db.get_item(item.id).await.unwrap().unwrap();
        assert_eq!(errored.status, ScoutStatus::Error);

        let changed = db
            .update_processed(
                item.id,
                "Retried OK",
                75,
                60,
                Some("Blog"),
                Some("2025-12-01"),
            )
            .await
            .unwrap();
        assert!(changed);
        let updated = db.get_item(item.id).await.unwrap().unwrap();
        assert_eq!(updated.status, ScoutStatus::Processed);
        assert_eq!(updated.title.as_deref(), Some("Retried OK"));
    }

    #[tokio::test]
    async fn rollback_processed_restores_original_metadata() {
        let db = test_db().await;
        let (item, _) = db
            .add_item("https://rollback.com", "other", None)
            .await
            .unwrap();
        db.set_title(item.id, "Original Title").await.unwrap();
        let original = db.get_item(item.id).await.unwrap().unwrap();

        db.update_processed(
            item.id,
            "AI Title",
            91,
            77,
            Some("Feed"),
            Some("2026-03-01"),
        )
        .await
        .unwrap();
        db.rollback_processed(&original).await.unwrap();

        let rolled_back = db.get_item(item.id).await.unwrap().unwrap();
        assert_eq!(rolled_back.status, ScoutStatus::Pending);
        assert_eq!(rolled_back.title.as_deref(), Some("Original Title"));
        assert_eq!(rolled_back.relevance, original.relevance);
        assert_eq!(rolled_back.quality, original.quality);
        assert_eq!(rolled_back.source_name, original.source_name);
        assert_eq!(rolled_back.date_processed, original.date_processed);
        assert_eq!(rolled_back.date_published, original.date_published);
    }

    #[tokio::test]
    async fn set_title_without_status_change() {
        let db = test_db().await;
        let (item, _) = db
            .add_item("https://title.com", "other", None)
            .await
            .unwrap();
        assert!(item.title.is_none());
        assert_eq!(item.status, ScoutStatus::Pending);

        db.set_title(item.id, "My Title").await.unwrap();
        let updated = db.get_item(item.id).await.unwrap().unwrap();
        assert_eq!(updated.title.as_deref(), Some("My Title"));
        assert_eq!(updated.status, ScoutStatus::Pending);
    }
}
