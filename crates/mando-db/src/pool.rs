//! Database connection pool and migration runner.

use std::path::Path;

use anyhow::{Context, Result};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;

/// Shared database handle. All crates receive an `Arc<Db>` from the gateway.
pub struct Db {
    pool: SqlitePool,
}

impl Db {
    /// Open (or create) `mando.db` at the given path, run migrations, return pool.
    pub async fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }

        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .busy_timeout(std::time::Duration::from_secs(5))
            .synchronous(sqlx::sqlite::SqliteSynchronous::Normal)
            .foreign_keys(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(8)
            .connect_with(options)
            .await
            .context("failed to open mando.db")?;

        let db = Self { pool };
        db.run_migrations().await?;
        Ok(db)
    }

    /// Open an in-memory database (for tests).
    pub async fn open_in_memory() -> Result<Self> {
        let options = SqliteConnectOptions::new()
            .filename(":memory:")
            .foreign_keys(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .context("failed to open in-memory DB")?;

        let db = Self { pool };
        db.run_migrations().await?;
        Ok(db)
    }

    /// Access the underlying sqlx pool.
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Run embedded migrations.
    async fn run_migrations(&self) -> Result<()> {
        // We embed migrations as raw SQL and run them manually with a version table,
        // because sqlx::migrate!() requires a build-time DATABASE_URL and offline mode
        // setup that adds CI complexity. This approach is simpler and equally safe.
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS _schema_version (
                version INTEGER PRIMARY KEY,
                applied_at TEXT NOT NULL DEFAULT (datetime('now'))
            )",
        )
        .execute(&self.pool)
        .await?;

        let current: i64 =
            sqlx::query_scalar("SELECT COALESCE(MAX(version), 0) FROM _schema_version")
                .fetch_one(&self.pool)
                .await?;

        for (version, sql) in MIGRATIONS {
            if *version > current {
                // PRAGMA foreign_keys is a no-op inside a transaction, so
                // disable FKs before the tx for migrations that need it.
                let needs_fk_off = sql.contains("PRAGMA foreign_keys = OFF");
                if needs_fk_off {
                    sqlx::query("PRAGMA foreign_keys = OFF")
                        .execute(&self.pool)
                        .await?;
                }

                // Strip PRAGMA foreign_keys statements from the SQL since
                // they're handled outside the transaction.
                let cleaned = if needs_fk_off {
                    sql.lines()
                        .filter(|l| {
                            let t = l.trim().to_lowercase();
                            !t.starts_with("pragma foreign_keys")
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                } else {
                    sql.to_string()
                };

                let mut tx = self.pool.begin().await?;
                sqlx::raw_sql(&cleaned)
                    .execute(&mut *tx)
                    .await
                    .with_context(|| format!("migration v{version} failed"))?;
                sqlx::query("INSERT INTO _schema_version (version) VALUES (?)")
                    .bind(*version)
                    .execute(&mut *tx)
                    .await?;
                tx.commit().await?;

                if needs_fk_off {
                    sqlx::query("PRAGMA foreign_keys = ON")
                        .execute(&self.pool)
                        .await?;
                }
                tracing::info!(version, "migration applied");
            }
        }

        Ok(())
    }
}

/// Embedded migrations. Each tuple: (version, SQL).
const MIGRATIONS: &[(i64, &str)] = &[
    (1, include_str!("../migrations/001_initial.sql")),
    (2, include_str!("../migrations/002_drop_linear.sql")),
    (3, include_str!("../migrations/003_timeline_dedup.sql")),
    (4, include_str!("../migrations/004_drop_branch.sql")),
    (5, include_str!("../migrations/005_workbenches.sql")),
    (6, include_str!("../migrations/006_audit_cleanup.sql")),
    (7, include_str!("../migrations/007_projects_table.sql")),
    (8, include_str!("../migrations/008_projects_full.sql")),
    (9, include_str!("../migrations/009_cleanup_fks.sql")),
    (10, include_str!("../migrations/010_rev_column.sql")),
    (
        11,
        include_str!("../migrations/011_remove_task_archived_at.sql"),
    ),
    (12, include_str!("../migrations/012_workbench_pinned.sql")),
    (13, include_str!("../migrations/013_session_resumed_at.sql")),
    (
        14,
        include_str!("../migrations/014_ask_history_sessions.sql"),
    ),
    (
        15,
        include_str!("../migrations/015_scout_research_runs.sql"),
    ),
    (
        16,
        include_str!("../migrations/016_scout_summary_article.sql"),
    ),
    (17, include_str!("../migrations/017_credential_email.sql")),
    (18, include_str!("../migrations/018_task_artifacts.sql")),
    (19, include_str!("../migrations/019_artifact_redesign.sql")),
    (
        20,
        include_str!("../migrations/020_workbench_last_activity.sql"),
    ),
    (
        21,
        include_str!("../migrations/021_workbench_pending_title.sql"),
    ),
    (22, include_str!("../migrations/022_task_planning.sql")),
    (
        23,
        include_str!("../migrations/023_task_workbench_not_null.sql"),
    ),
    (24, include_str!("../migrations/024_task_no_auto_merge.sql")),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn open_in_memory_succeeds() {
        let db = Db::open_in_memory().await.unwrap();
        // Verify tables exist by querying them.
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM cc_sessions")
            .fetch_one(db.pool())
            .await
            .unwrap();
        assert_eq!(count, 0);
    }

    #[tokio::test]
    async fn migrations_are_idempotent() {
        let db = Db::open_in_memory().await.unwrap();
        // Running migrations again should be a no-op.
        db.run_migrations().await.unwrap();
        let version: i64 =
            sqlx::query_scalar("SELECT COALESCE(MAX(version), 0) FROM _schema_version")
                .fetch_one(db.pool())
                .await
                .unwrap();
        assert_eq!(version, MIGRATIONS.last().unwrap().0);
    }
}
