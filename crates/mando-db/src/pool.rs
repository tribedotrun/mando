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
            std::fs::create_dir_all(parent)?;
        }

        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .busy_timeout(std::time::Duration::from_secs(5))
            .synchronous(sqlx::sqlite::SqliteSynchronous::Normal);

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
        let options = SqliteConnectOptions::new().filename(":memory:");

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
                let mut tx = self.pool.begin().await?;
                sqlx::raw_sql(sql)
                    .execute(&mut *tx)
                    .await
                    .with_context(|| format!("migration v{version} failed"))?;
                sqlx::query("INSERT INTO _schema_version (version) VALUES (?)")
                    .bind(*version)
                    .execute(&mut *tx)
                    .await?;
                tx.commit().await?;
                tracing::info!(version, "migration applied");
            }
        }

        Ok(())
    }
}

/// Embedded migrations. Each tuple: (version, SQL).
const MIGRATIONS: &[(i64, &str)] = &[(1, include_str!("../migrations/001_initial.sql"))];

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
