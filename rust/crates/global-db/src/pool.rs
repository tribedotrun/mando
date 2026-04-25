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

    /// Regression: the FK-off PRAGMA and the migration transaction must
    /// execute on the same pooled connection. If they land on different
    /// connections, a migration that drops a table referenced by another
    /// table's FK fails with `FOREIGN KEY constraint failed (19)` on a
    /// connection that never saw the OFF. We simulate that scenario with a
    /// file-backed pool (max_connections > 1) and an ad-hoc migration SQL.
    #[tokio::test]
    async fn fk_off_pragma_pins_to_migration_connection() {
        use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

        let db_path = std::env::temp_dir().join(format!(
            "mando-pool-fk-test-{}-{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let options = SqliteConnectOptions::new()
            .filename(&db_path)
            .create_if_missing(true)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(options)
            .await
            .unwrap();

        sqlx::raw_sql(
            "CREATE TABLE parent (id INTEGER PRIMARY KEY);
             CREATE TABLE child (
                id INTEGER PRIMARY KEY,
                parent_id INTEGER REFERENCES parent(id)
             );
             INSERT INTO parent(id) VALUES (1);
             INSERT INTO child(id, parent_id) VALUES (1, 1);",
        )
        .execute(&pool)
        .await
        .unwrap();

        // Warm multiple connections so the pool has >1 ready. Without
        // connection pinning, the PRAGMA would likely land on one and the
        // transaction on another.
        let mut warmups = Vec::new();
        for _ in 0..4 {
            warmups.push(pool.acquire().await.unwrap());
        }
        drop(warmups);

        // Drive the same flow as `run_migrations`: acquire a connection,
        // PRAGMA OFF on it, run a tx that drops the referenced table,
        // PRAGMA ON, release.
        let mut conn = pool.acquire().await.unwrap();
        sqlx::query("PRAGMA foreign_keys = OFF")
            .execute(&mut *conn)
            .await
            .unwrap();
        let mut tx = conn.begin().await.unwrap();
        sqlx::raw_sql(
            "CREATE TABLE parent_new (id INTEGER PRIMARY KEY, extra TEXT);
             INSERT INTO parent_new (id) SELECT id FROM parent;
             DROP TABLE parent;
             ALTER TABLE parent_new RENAME TO parent;",
        )
        .execute(&mut *tx)
        .await
        .expect("DROP TABLE must succeed when PRAGMA and tx share a connection");
        tx.commit().await.unwrap();
        sqlx::query("PRAGMA foreign_keys = ON")
            .execute(&mut *conn)
            .await
            .unwrap();
        drop(conn);

        // Child row still resolves and new parent has the extra column.
        let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM child WHERE parent_id = 1")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 1);

        drop(pool);
        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_file(format!("{}-shm", db_path.display()));
        let _ = std::fs::remove_file(format!("{}-wal", db_path.display()));
    }

    /// Regression: if a migration errors after `PRAGMA foreign_keys = OFF`,
    /// `run_migrations` must still restore `foreign_keys = ON` on the
    /// acquired connection before it returns to the pool. Otherwise the
    /// connection silently disables FK enforcement for the next caller.
    #[tokio::test]
    async fn fk_state_restored_even_when_migration_fails() {
        use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

        let db_path = std::env::temp_dir().join(format!(
            "mando-pool-fk-restore-{}-{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let options = SqliteConnectOptions::new()
            .filename(&db_path)
            .create_if_missing(true)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1) // force the same conn to be reused
            .connect_with(options)
            .await
            .unwrap();

        // Simulate the FK-off wrapper with a migration body that always
        // errors — mirrors run_migrations semantics.
        let mut conn = pool.acquire().await.unwrap();
        sqlx::query("PRAGMA foreign_keys = OFF")
            .execute(&mut *conn)
            .await
            .unwrap();
        let result: std::result::Result<(), sqlx::Error> = async {
            let mut tx = conn.begin().await?;
            sqlx::raw_sql("INSERT INTO nonexistent_table VALUES (1)")
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;
            Ok(())
        }
        .await;
        global_infra::best_effort!(
            sqlx::query("PRAGMA foreign_keys = ON")
                .execute(&mut *conn)
                .await,
            "test restore foreign_keys=ON"
        );
        drop(conn);
        assert!(result.is_err(), "migration body must fail for this test");

        // Next caller on the same (now-recycled) connection should see
        // foreign_keys = ON, not the leaked OFF state.
        let fk_state: i64 = sqlx::query_scalar("PRAGMA foreign_keys")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(
            fk_state, 1,
            "pool connection must not leak foreign_keys=OFF after a failed migration"
        );

        drop(pool);
        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_file(format!("{}-shm", db_path.display()));
        let _ = std::fs::remove_file(format!("{}-wal", db_path.display()));
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

    /// Step E3 guardrail — the `MIGRATIONS` array and the `migrations/*.sql`
    /// directory must stay in lockstep. Missing a file on disk or forgetting
    /// to register one in the array is a silent correctness bug at runtime.
    #[test]
    fn migrations_array_matches_migrations_directory() {
        use std::collections::{BTreeSet, HashMap};

        let migrations_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
        assert!(
            migrations_dir.is_dir(),
            "migrations/ directory not found at {}",
            migrations_dir.display()
        );

        // Parse (version, filename) from each <NNN>_<name>.sql on disk.
        let mut disk_by_version: HashMap<i64, String> = HashMap::new();
        for entry in std::fs::read_dir(&migrations_dir).expect("read migrations dir") {
            let entry = entry.expect("read entry");
            let path = entry.path();
            if !path.is_file() || path.extension().and_then(|e| e.to_str()) != Some("sql") {
                continue;
            }
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .expect("utf-8 filename");
            let stem = name.strip_suffix(".sql").expect("sql suffix");
            let (num_str, _rest) = stem
                .split_once('_')
                .unwrap_or_else(|| panic!("migration filename must be <NNN>_<name>.sql: {name}"));
            let version: i64 = num_str
                .parse()
                .unwrap_or_else(|_| panic!("migration filename prefix must be numeric: {name}"));
            if let Some(prev) = disk_by_version.insert(version, name.to_string()) {
                panic!("duplicate migration version {version}: {prev} and {name}");
            }
        }

        let disk_versions: BTreeSet<i64> = disk_by_version.keys().copied().collect();

        // Detect duplicate versions in MIGRATIONS explicitly — collecting
        // straight into a BTreeSet would silently drop them.
        let mut array_versions: BTreeSet<i64> = BTreeSet::new();
        for (version, _) in MIGRATIONS {
            assert!(
                array_versions.insert(*version),
                "duplicate version {version} in MIGRATIONS array"
            );
        }

        let missing_from_array: Vec<&i64> = disk_versions.difference(&array_versions).collect();
        let missing_from_disk: Vec<&i64> = array_versions.difference(&disk_versions).collect();

        let mut err = String::new();
        if !missing_from_array.is_empty() {
            err.push_str(&format!(
                "\nmigrations/ contains .sql files not in MIGRATIONS array: versions {missing_from_array:?}\n\
                 Each new migration must be registered in rust/crates/global-db/src/pool.rs MIGRATIONS const.\n"
            ));
        }
        if !missing_from_disk.is_empty() {
            err.push_str(&format!(
                "\nMIGRATIONS array has versions without matching migrations/*.sql: {missing_from_disk:?}\n\
                 Either add the file or remove the array entry.\n"
            ));
        }
        assert!(err.is_empty(), "{err}");

        // Additionally: versions must be monotonic starting at 1.
        let expected: Vec<i64> = (1..=array_versions.len() as i64).collect();
        let actual: Vec<i64> = array_versions.iter().copied().collect();
        assert_eq!(
            actual, expected,
            "MIGRATIONS versions must be monotonically numbered starting at 1 with no gaps"
        );
    }
}
