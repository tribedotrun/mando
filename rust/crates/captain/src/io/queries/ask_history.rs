//! Ask history queries -- multi-session Q&A storage.

use anyhow::Result;
use sqlx::SqlitePool;

use crate::AskHistoryEntry;

#[derive(sqlx::FromRow)]
struct AskHistoryRow {
    ask_id: String,
    session_id: String,
    role: String,
    content: String,
    timestamp: String,
}

impl AskHistoryRow {
    fn into_entry(self) -> AskHistoryEntry {
        AskHistoryEntry {
            ask_id: self.ask_id,
            session_id: self.session_id,
            role: self.role,
            content: self.content,
            timestamp: self.timestamp,
        }
    }
}

/// Append an entry to a task's ask history.
pub async fn append(pool: &SqlitePool, task_id: i64, entry: &AskHistoryEntry) -> Result<()> {
    sqlx::query(
        "INSERT INTO ask_history (task_id, ask_id, session_id, role, content, timestamp)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
    )
    .bind(task_id)
    .bind(&entry.ask_id)
    .bind(&entry.session_id)
    .bind(&entry.role)
    .bind(&entry.content)
    .bind(&entry.timestamp)
    .execute(pool)
    .await?;
    Ok(())
}

/// Load all ask history for a task, ordered chronologically.
pub async fn load(pool: &SqlitePool, task_id: i64) -> Result<Vec<AskHistoryEntry>> {
    let rows: Vec<AskHistoryRow> = sqlx::query_as(
        "SELECT ask_id, session_id, role, content, timestamp
         FROM ask_history WHERE task_id = ? ORDER BY timestamp ASC",
    )
    .bind(task_id)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|r| r.into_entry()).collect())
}

/// Replace the `'pending'` placeholder with the real CC session id on every
/// row in the given `(task_id, ask_id)` group. The HTTP routes persist a
/// question (and any retry error) before the CC call returns, so the row's
/// `session_id` is initialised to `'pending'`. Once the call returns we know
/// the real id and can flip those rows; rows that already carry a real id
/// (or `'legacy'` from migration 014) are left alone.
pub async fn update_pending_session_id(
    pool: &SqlitePool,
    task_id: i64,
    ask_id: &str,
    real_session_id: &str,
) -> Result<()> {
    sqlx::query(
        "UPDATE ask_history
         SET session_id = ?1
         WHERE task_id = ?2 AND ask_id = ?3 AND session_id = 'pending'",
    )
    .bind(real_session_id)
    .bind(task_id)
    .bind(ask_id)
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use global_types::now_rfc3339;

    async fn test_pool() -> SqlitePool {
        let db = global_db::Db::open_in_memory().await.unwrap();
        let project_id = settings::projects::upsert(db.pool(), "test", "", None)
            .await
            .unwrap();
        let wb_id = crate::io::test_support::seed_workbench(db.pool(), project_id).await;
        sqlx::query(
            "INSERT INTO tasks (id, title, project_id, workbench_id, status, created_at,
                 last_activity_at, session_ids, no_pr, rev)
             VALUES (1, 'test task', ?, ?, 'awaiting-review', ?, ?, '{}', 0, 1)",
        )
        .bind(project_id)
        .bind(wb_id)
        .bind(now_rfc3339())
        .bind(now_rfc3339())
        .execute(db.pool())
        .await
        .unwrap();
        db.pool().clone()
    }

    fn entry(ask_id: &str, session_id: &str, role: &str) -> AskHistoryEntry {
        AskHistoryEntry {
            ask_id: ask_id.into(),
            session_id: session_id.into(),
            role: role.into(),
            content: format!("{role} content"),
            timestamp: now_rfc3339(),
        }
    }

    /// Regression for #126: question rows persisted before the CC session id
    /// is known carry `'pending'`. Once the assistant row arrives with the
    /// real id, calling `update_pending_session_id` flips the question (and
    /// any pending error rows in the same `ask_id` group) to that id.
    #[tokio::test]
    async fn update_pending_session_id_backfills_group() {
        let pool = test_pool().await;
        let real = "abc123-real-session";

        append(&pool, 1, &entry("ask-A", "pending", "human"))
            .await
            .unwrap();
        append(&pool, 1, &entry("ask-A", "pending", "error"))
            .await
            .unwrap();
        append(&pool, 1, &entry("ask-A", real, "assistant"))
            .await
            .unwrap();

        // Pre-condition: writes landed with the placeholder.
        let rows = load(&pool, 1).await.unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].session_id, "pending");
        assert_eq!(rows[1].session_id, "pending");
        assert_eq!(rows[2].session_id, real);

        update_pending_session_id(&pool, 1, "ask-A", real)
            .await
            .unwrap();

        let rows = load(&pool, 1).await.unwrap();
        assert!(rows.iter().all(|r| r.session_id == real));
    }

    /// Backfill must scope to the named `(task_id, ask_id)` group: a stale
    /// pending row from a prior conversation in the same task is left alone.
    #[tokio::test]
    async fn update_pending_session_id_scopes_to_ask_id() {
        let pool = test_pool().await;
        let real = "session-uuid-A";

        append(&pool, 1, &entry("ask-A", "pending", "human"))
            .await
            .unwrap();
        append(&pool, 1, &entry("ask-A", real, "assistant"))
            .await
            .unwrap();
        append(&pool, 1, &entry("ask-B", "pending", "human"))
            .await
            .unwrap();

        update_pending_session_id(&pool, 1, "ask-A", real)
            .await
            .unwrap();

        let rows = load(&pool, 1).await.unwrap();
        let ask_a_pending = rows
            .iter()
            .filter(|r| r.ask_id == "ask-A" && r.session_id == "pending")
            .count();
        let ask_b_pending = rows
            .iter()
            .filter(|r| r.ask_id == "ask-B" && r.session_id == "pending")
            .count();
        assert_eq!(ask_a_pending, 0);
        assert_eq!(ask_b_pending, 1);
    }

    /// Migration 037 backfills historical `'pending'` rows from the matching
    /// assistant row in the same group. `'legacy'` rows have no recoverable
    /// id and stay as-is; pending rows whose group has no assistant id stay
    /// pending too.
    #[tokio::test]
    async fn migration_037_backfills_existing_pending_rows() {
        let pool = test_pool().await;
        let real = "real-uuid-37";

        // Group with assistant: should be backfilled.
        append(&pool, 1, &entry("ask-A", "pending", "human"))
            .await
            .unwrap();
        append(&pool, 1, &entry("ask-A", real, "assistant"))
            .await
            .unwrap();

        // Group without assistant: stays pending.
        append(&pool, 1, &entry("ask-B", "pending", "human"))
            .await
            .unwrap();

        // Legacy rows are off-limits.
        append(&pool, 1, &entry("legacy", "legacy", "human"))
            .await
            .unwrap();

        // Re-run the migration body against the existing pool. Migrations
        // are idempotent inside `Db::open_in_memory`, but the rows we just
        // inserted weren't present at first-open time, so we rerun the
        // migration SQL manually here to assert the backfill behavior.
        let sql = include_str!(
            "../../../../global-db/migrations/037_backfill_ask_history_pending_session.sql"
        );
        sqlx::raw_sql(sql).execute(&pool).await.unwrap();

        let rows = load(&pool, 1).await.unwrap();
        let ask_a_human = rows
            .iter()
            .find(|r| r.ask_id == "ask-A" && r.role == "human")
            .unwrap();
        assert_eq!(ask_a_human.session_id, real);

        let ask_b_human = rows
            .iter()
            .find(|r| r.ask_id == "ask-B" && r.role == "human")
            .unwrap();
        assert_eq!(ask_b_human.session_id, "pending");

        let legacy = rows.iter().find(|r| r.ask_id == "legacy").unwrap();
        assert_eq!(legacy.session_id, "legacy");
    }
}
