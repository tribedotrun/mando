use crate::Workbench;
use anyhow::Result;
use sqlx::SqlitePool;

#[derive(sqlx::FromRow)]
struct Row {
    id: i64,
    project_id: i64,
    project: String,
    worktree: String,
    title: String,
    created_at: String,
    last_activity_at: String,
    pinned_at: Option<String>,
    archived_at: Option<String>,
    deleted_at: Option<String>,
    rev: i64,
}

impl Row {
    fn into_workbench(self) -> Workbench {
        Workbench {
            id: self.id,
            project_id: self.project_id,
            project: self.project,
            worktree: self.worktree,
            title: self.title,
            created_at: self.created_at,
            last_activity_at: self.last_activity_at,
            pinned_at: self.pinned_at,
            archived_at: self.archived_at,
            deleted_at: self.deleted_at,
            rev: self.rev,
        }
    }
}

const SELECT: &str = "\
    w.id, w.project_id, p.name AS project, w.worktree, w.title, \
    w.created_at, COALESCE(w.last_activity_at, w.created_at) AS last_activity_at, \
    w.pinned_at, w.archived_at, w.deleted_at, w.rev";

fn select_sql() -> &'static str {
    static SQL: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    SQL.get_or_init(|| {
        format!("SELECT {SELECT} FROM workbenches w JOIN projects p ON p.id = w.project_id")
    })
}

pub async fn insert(pool: &SqlitePool, wb: &Workbench) -> Result<i64> {
    let result = sqlx::query(
        "INSERT INTO workbenches (project_id, worktree, title, created_at, last_activity_at) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(wb.project_id)
    .bind(&wb.worktree)
    .bind(&wb.title)
    .bind(&wb.created_at)
    .bind(&wb.last_activity_at)
    .execute(pool)
    .await?;
    Ok(result.last_insert_rowid())
}

/// Insert a workbench inside an existing transaction. Used by atomic
/// task+workbench creation so both INSERTs commit together.
pub(crate) async fn insert_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    project_id: i64,
    worktree: &str,
    title: &str,
    now: &str,
) -> Result<i64> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO workbenches (project_id, worktree, title, created_at, last_activity_at) \
         VALUES (?, ?, ?, ?, ?) RETURNING id",
    )
    .bind(project_id)
    .bind(worktree)
    .bind(title)
    .bind(now)
    .bind(now)
    .fetch_one(&mut **tx)
    .await?;
    Ok(id)
}

/// Bump `last_activity_at` to now and increment `rev`. Returns `true` if a row
/// was updated. Skips archived/deleted rows so stale hook callbacks can't
/// resurrect them in the sidebar.
pub async fn touch_activity(pool: &SqlitePool, id: i64) -> Result<bool> {
    let now = global_types::now_rfc3339();
    let result = sqlx::query(
        "UPDATE workbenches SET last_activity_at = ?, rev = rev + 1 \
         WHERE id = ? AND archived_at IS NULL AND deleted_at IS NULL",
    )
    .bind(&now)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn find_by_id(pool: &SqlitePool, id: i64) -> Result<Option<Workbench>> {
    let sql = format!("{} WHERE w.id = ?", select_sql());
    let row: Option<Row> = sqlx::query_as(&sql).bind(id).fetch_optional(pool).await?;
    Ok(row.map(|r| r.into_workbench()))
}

pub async fn load_active(pool: &SqlitePool) -> Result<Vec<Workbench>> {
    let sql = format!(
        "{} WHERE w.archived_at IS NULL AND w.deleted_at IS NULL",
        select_sql()
    );
    let rows: Vec<Row> = sqlx::query_as(&sql).fetch_all(pool).await?;
    Ok(rows.into_iter().map(|r| r.into_workbench()).collect())
}

pub async fn load_all(pool: &SqlitePool) -> Result<Vec<Workbench>> {
    let sql = format!("{} WHERE w.deleted_at IS NULL", select_sql());
    let rows: Vec<Row> = sqlx::query_as(&sql).fetch_all(pool).await?;
    Ok(rows.into_iter().map(|r| r.into_workbench()).collect())
}

pub async fn load_archived_only(pool: &SqlitePool) -> Result<Vec<Workbench>> {
    let sql = format!(
        "{} WHERE w.archived_at IS NOT NULL AND w.deleted_at IS NULL",
        select_sql()
    );
    let rows: Vec<Row> = sqlx::query_as(&sql).fetch_all(pool).await?;
    Ok(rows.into_iter().map(|r| r.into_workbench()).collect())
}

pub async fn archive(pool: &SqlitePool, id: i64) -> Result<bool> {
    let now = global_types::now_rfc3339();
    let result = sqlx::query(
        "UPDATE workbenches SET archived_at = ?, pinned_at = NULL, rev = rev + 1 WHERE id = ?",
    )
    .bind(&now)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn unarchive(pool: &SqlitePool, id: i64) -> Result<bool> {
    let result =
        sqlx::query("UPDATE workbenches SET archived_at = NULL, rev = rev + 1 WHERE id = ?")
            .bind(id)
            .execute(pool)
            .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn mark_deleted(pool: &SqlitePool, id: i64) -> Result<bool> {
    let now = global_types::now_rfc3339();
    let result = sqlx::query("UPDATE workbenches SET deleted_at = ?, rev = rev + 1 WHERE id = ?")
        .bind(&now)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn pin(pool: &SqlitePool, id: i64) -> Result<bool> {
    let now = global_types::now_rfc3339();
    let result = sqlx::query(
        "UPDATE workbenches SET pinned_at = ?, rev = rev + 1 \
         WHERE id = ? AND archived_at IS NULL AND deleted_at IS NULL",
    )
    .bind(&now)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn unpin(pool: &SqlitePool, id: i64) -> Result<bool> {
    let result = sqlx::query("UPDATE workbenches SET pinned_at = NULL, rev = rev + 1 WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn update_title(pool: &SqlitePool, id: i64, title: &str) -> Result<bool> {
    let result = sqlx::query("UPDATE workbenches SET title = ?, rev = rev + 1 WHERE id = ?")
        .bind(title)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn stale_archived(pool: &SqlitePool, older_than_days: i64) -> Result<Vec<Workbench>> {
    let sql = format!(
        "{} WHERE w.archived_at IS NOT NULL \
           AND w.deleted_at IS NULL \
           AND datetime(w.archived_at) <= datetime('now', ? || ' days')",
        select_sql()
    );
    let offset = format!("-{older_than_days}");
    let rows: Vec<Row> = sqlx::query_as(&sql).bind(&offset).fetch_all(pool).await?;
    Ok(rows.into_iter().map(|r| r.into_workbench()).collect())
}
