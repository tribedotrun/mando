//! Project queries.

use anyhow::{Context, Result};
use serde::de::DeserializeOwned;
use sqlx::SqlitePool;

const SELECT_COLS: &str = "\
    id, name, path, github_repo, aliases, hooks, worker_preamble, \
    check_command, logo, scout_summary, classify_rules, created_at, updated_at";

/// A project row from the projects table.
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct ProjectRow {
    pub id: i64,
    pub name: String,
    pub path: String,
    pub github_repo: Option<String>,
    /// JSON array of alias strings.
    pub aliases: String,
    /// JSON object of hook name -> command.
    pub hooks: String,
    pub worker_preamble: String,
    pub check_command: String,
    pub logo: Option<String>,
    pub scout_summary: String,
    /// JSON array of classify rule objects.
    pub classify_rules: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Find a project by name.
pub async fn find_by_name(pool: &SqlitePool, name: &str) -> Result<Option<ProjectRow>> {
    let sql = format!("SELECT {SELECT_COLS} FROM projects WHERE name = ?");
    let row: Option<ProjectRow> = sqlx::query_as(&sql).bind(name).fetch_optional(pool).await?;
    Ok(row)
}

/// Find a project by path.
pub async fn find_by_path(pool: &SqlitePool, path: &str) -> Result<Option<ProjectRow>> {
    let sql = format!("SELECT {SELECT_COLS} FROM projects WHERE path = ?");
    let row: Option<ProjectRow> = sqlx::query_as(&sql).bind(path).fetch_optional(pool).await?;
    Ok(row)
}

/// Find a project by ID.
pub async fn find_by_id(pool: &SqlitePool, id: i64) -> Result<Option<ProjectRow>> {
    let sql = format!("SELECT {SELECT_COLS} FROM projects WHERE id = ?");
    let row: Option<ProjectRow> = sqlx::query_as(&sql).bind(id).fetch_optional(pool).await?;
    Ok(row)
}

/// List all projects.
pub async fn list(pool: &SqlitePool) -> Result<Vec<ProjectRow>> {
    let sql = format!("SELECT {SELECT_COLS} FROM projects ORDER BY name");
    let rows: Vec<ProjectRow> = sqlx::query_as(&sql).fetch_all(pool).await?;
    Ok(rows)
}

/// Resolve a project by name or alias. Checks name first, then JSON aliases array.
pub async fn resolve(pool: &SqlitePool, name_or_alias: &str) -> Result<Option<ProjectRow>> {
    let sql = format!(
        "SELECT {SELECT_COLS} FROM projects \
         WHERE LOWER(name) = LOWER(?1) \
            OR EXISTS (SELECT 1 FROM json_each(aliases) WHERE LOWER(value) = LOWER(?1)) \
         LIMIT 1"
    );
    let row: Option<ProjectRow> = sqlx::query_as(&sql)
        .bind(name_or_alias)
        .fetch_optional(pool)
        .await?;
    Ok(row)
}

/// Upsert a project (insert or update all fields).
pub async fn upsert(
    pool: &SqlitePool,
    name: &str,
    path: &str,
    github_repo: Option<&str>,
) -> Result<i64> {
    let now = global_types::now_rfc3339();
    sqlx::query(
        "INSERT INTO projects (name, path, github_repo, created_at, updated_at) VALUES (?, ?, ?, ?, ?)
         ON CONFLICT(name) DO UPDATE SET path = excluded.path, github_repo = excluded.github_repo, updated_at = excluded.updated_at",
    )
    .bind(name)
    .bind(path)
    .bind(github_repo)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;

    let id: i64 = sqlx::query_scalar("SELECT id FROM projects WHERE name = ?")
        .bind(name)
        .fetch_one(pool)
        .await?;
    Ok(id)
}

/// Full upsert with all config fields.
pub async fn upsert_full(pool: &SqlitePool, row: &ProjectRow) -> Result<i64> {
    let now = global_types::now_rfc3339();
    sqlx::query(
        "INSERT INTO projects (name, path, github_repo, aliases, hooks, worker_preamble, \
         check_command, logo, scout_summary, classify_rules, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11) \
         ON CONFLICT(name) DO UPDATE SET \
         path=excluded.path, github_repo=excluded.github_repo, aliases=excluded.aliases, \
         hooks=excluded.hooks, worker_preamble=excluded.worker_preamble, \
         check_command=excluded.check_command, logo=excluded.logo, \
         scout_summary=excluded.scout_summary, classify_rules=excluded.classify_rules, \
         updated_at=excluded.updated_at",
    )
    .bind(&row.name)
    .bind(&row.path)
    .bind(&row.github_repo)
    .bind(&row.aliases)
    .bind(&row.hooks)
    .bind(&row.worker_preamble)
    .bind(&row.check_command)
    .bind(&row.logo)
    .bind(&row.scout_summary)
    .bind(&row.classify_rules)
    .bind(&now)
    .execute(pool)
    .await?;

    let id: i64 = sqlx::query_scalar("SELECT id FROM projects WHERE name = ?")
        .bind(&row.name)
        .fetch_one(pool)
        .await?;
    Ok(id)
}

/// Update specific fields on a project.
pub async fn update(pool: &SqlitePool, id: i64, row: &ProjectRow) -> Result<bool> {
    let now = global_types::now_rfc3339();
    let result = sqlx::query(
        "UPDATE projects SET name=?, path=?, github_repo=?, aliases=?, hooks=?, \
         worker_preamble=?, check_command=?, logo=?, scout_summary=?, \
         classify_rules=?, updated_at=? WHERE id=?",
    )
    .bind(&row.name)
    .bind(&row.path)
    .bind(&row.github_repo)
    .bind(&row.aliases)
    .bind(&row.hooks)
    .bind(&row.worker_preamble)
    .bind(&row.check_command)
    .bind(&row.logo)
    .bind(&row.scout_summary)
    .bind(&row.classify_rules)
    .bind(&now)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Delete a project by ID.
pub async fn delete(pool: &SqlitePool, id: i64) -> Result<bool> {
    let result = sqlx::query("DELETE FROM projects WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

// ── Conversion helpers ──────────────────────────────────────────────────────

fn parse_project_json<T>(row: &ProjectRow, field_name: &str, raw: &str) -> Result<T>
where
    T: DeserializeOwned,
{
    serde_json::from_str(raw).with_context(|| {
        format!(
            "project {} has invalid {field_name} JSON in persisted settings",
            row.name
        )
    })
}

/// Convert a DB row to the config-layer ProjectConfig type.
pub fn row_to_config(row: &ProjectRow) -> Result<crate::config::settings::ProjectConfig> {
    Ok(crate::config::settings::ProjectConfig {
        name: row.name.clone(),
        path: row.path.clone(),
        github_repo: row.github_repo.clone(),
        aliases: parse_project_json(row, "aliases", &row.aliases)?,
        hooks: parse_project_json(row, "hooks", &row.hooks)?,
        worker_preamble: row.worker_preamble.clone(),
        check_command: row.check_command.clone(),
        logo: row.logo.clone(),
        scout_summary: row.scout_summary.clone(),
        classify_rules: parse_project_json(row, "classify_rules", &row.classify_rules)?,
    })
}

/// Convert a config-layer ProjectConfig to a DB row for upsert.
///
/// Fail-fast: returns `Err` on serde failures. Previously this function
/// silently substituted empty `[]`/`{}` defaults, which would
/// round-trip back from DB as if the user had cleared the field — a
/// data-loss event mirroring (and violating) CLAUDE.md's "Persisted
/// project JSON reads surface corruption" invariant.
pub fn config_to_row(
    pc: &crate::config::settings::ProjectConfig,
) -> Result<ProjectRow, serde_json::Error> {
    Ok(ProjectRow {
        id: 0,
        name: pc.name.clone(),
        path: pc.path.clone(),
        github_repo: pc.github_repo.clone(),
        aliases: serde_json::to_string(&pc.aliases)?,
        hooks: serde_json::to_string(&pc.hooks)?,
        worker_preamble: pc.worker_preamble.clone(),
        check_command: pc.check_command.clone(),
        logo: pc.logo.clone(),
        scout_summary: pc.scout_summary.clone(),
        classify_rules: serde_json::to_string(&pc.classify_rules)?,
        created_at: String::new(),
        updated_at: String::new(),
    })
}

/// Load all projects from DB and populate config.captain.projects.
/// Called at daemon startup to make the DB the source of truth.
pub async fn load_into_config(
    pool: &SqlitePool,
    config: &mut crate::config::settings::Config,
) -> Result<()> {
    let rows = list(pool).await?;
    config.captain.projects.clear();
    for row in &rows {
        let pc = row_to_config(row)?;
        config.captain.projects.insert(pc.path.clone(), pc);
    }
    Ok(())
}

/// Load all projects from DB into the in-memory config cache,
/// backfill missing logos, and persist detected logos back to DB.
pub async fn startup_sync(
    pool: &SqlitePool,
    config: &mut crate::config::settings::Config,
) -> Result<()> {
    load_into_config(pool, config).await?;
    if crate::io::logo::backfill_project_logos(config) {
        for pc in config.captain.projects.values() {
            if pc.logo.is_some() {
                let row = config_to_row(pc).with_context(|| {
                    format!("failed to serialize project {} for logo backfill", pc.name)
                })?;
                if let Err(e) = upsert_full(pool, &row).await {
                    tracing::warn!(
                        module = "settings",
                        project = %pc.name,
                        error = %e,
                        "failed to persist backfilled project logo",
                    );
                }
            }
        }
    }
    Ok(())
}
