//! Database connection pool and migration runner.

use std::path::Path;

use anyhow::{Context, Result};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Acquire, SqlitePool};

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
                // PRAGMA foreign_keys is a no-op inside a transaction AND is
                // scoped to a single connection. The pool may hand different
                // queries to different connections, so the OFF/tx/ON triple
                // must all run on the same acquired connection — otherwise a
                // migration that drops a table with incoming FK references
                // fails with FOREIGN KEY constraint (19) on an unrelated
                // pool connection that still has FKs ON.
                let needs_fk_off = sql.contains("PRAGMA foreign_keys = OFF");

                // Strip PRAGMA foreign_keys statements from the SQL since
                // they're handled outside the transaction (a transaction is
                // a no-op context for this PRAGMA in SQLite).
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

                let mut conn = self.pool.acquire().await?;
                if needs_fk_off {
                    sqlx::query("PRAGMA foreign_keys = OFF")
                        .execute(&mut *conn)
                        .await?;
                }

                // Run the migration body inside a block so we can ALWAYS
                // attempt to restore FK enforcement before the connection
                // returns to the pool — even on commit failure, mid-tx
                // error, or a panicked early return. Leaking `foreign_keys
                // = OFF` into a pooled connection silently disables FK
                // checks for whoever acquires it next.
                let migration_result: Result<()> = async {
                    let mut tx = conn.begin().await?;
                    sqlx::raw_sql(&cleaned)
                        .execute(&mut *tx)
                        .await
                        .with_context(|| format!("migration v{version} failed"))?;
                    sqlx::query("INSERT INTO _schema_version (version) VALUES (?)")
                        .bind(*version)
                        .execute(&mut *tx)
                        .await?;
                    tx.commit().await?;
                    Ok(())
                }
                .await;

                if needs_fk_off {
                    // A pool connection stuck with FK OFF is a silent
                    // correctness bug, worse than a dropped PRAGMA error
                    // that the surrounding migration_result will almost
                    // always surface anyway — route the restore through
                    // best_effort! so the log still carries a breadcrumb
                    // if the restore itself fails.
                    global_infra::best_effort!(
                        sqlx::query("PRAGMA foreign_keys = ON")
                            .execute(&mut *conn)
                            .await,
                        "restore foreign_keys=ON after migration"
                    );
                }
                drop(conn);

                migration_result?;
                tracing::info!(module = "global-db-pool", version, "migration applied");
            }
        }

        Ok(())
    }
}

// Embedded migrations. Each tuple: (version, SQL).
// We embed migrations as raw SQL and run them manually with a version table,
// because sqlx::migrate!() requires a build-time DATABASE_URL and offline mode
// setup that adds CI complexity. This approach is simpler and equally safe.
// Generated by rust/crates/global-db/build.rs from migrations/*.sql
include!(concat!(env!("OUT_DIR"), "/migrations.rs"));
