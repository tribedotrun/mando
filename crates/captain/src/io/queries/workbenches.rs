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

pub async fn find_by_worktree(pool: &SqlitePool, worktree: &str) -> Result<Option<Workbench>> {
    let sql = format!("{} WHERE w.worktree = ?", select_sql());
    let row: Option<Row> = sqlx::query_as(&sql)
        .bind(worktree)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|r| r.into_workbench()))
}

pub async fn load_by_project(pool: &SqlitePool, project_id: i64) -> Result<Vec<Workbench>> {
    let sql = format!(
        "{} WHERE w.project_id = ? AND w.archived_at IS NULL AND w.deleted_at IS NULL",
        select_sql()
    );
    let rows: Vec<Row> = sqlx::query_as(&sql)
        .bind(project_id)
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(|r| r.into_workbench()).collect())
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

/// Archive workbenches whose task is in a terminal state and older than `grace_secs`.
pub async fn archive_terminal(pool: &SqlitePool, grace_secs: u64) -> Result<usize> {
    let cutoff = time::OffsetDateTime::now_utc() - time::Duration::seconds(grace_secs as i64);
    let cutoff_str = cutoff
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let now_str = global_types::now_rfc3339();

    let result = sqlx::query(
        "UPDATE workbenches SET archived_at = ?, rev = rev + 1
         WHERE archived_at IS NULL AND deleted_at IS NULL AND pinned_at IS NULL
           AND id IN (
             SELECT workbench_id FROM tasks
             WHERE workbench_id IS NOT NULL
             GROUP BY workbench_id
             HAVING
               COUNT(CASE WHEN status NOT IN ('merged','completed-no-pr','canceled') THEN 1 END) = 0
               AND MAX(datetime(COALESCE(last_activity_at, created_at))) <= datetime(?)
           )",
    )
    .bind(&now_str)
    .bind(&cutoff_str)
    .execute(pool)
    .await?;

    let archived = result.rows_affected() as usize;
    if archived > 0 {
        tracing::info!(
            module = "workbench",
            archived,
            "terminal workbenches archived"
        );
    }
    Ok(archived)
}

// ── Pending auto-title ──────────────────────────────────────────────

/// Row returned by the pending-title query. Carries the CC session ID
/// needed for auto-titling without leaking it into the public Workbench type.
#[derive(Debug, sqlx::FromRow)]
pub struct PendingTitleRow {
    pub id: i64,
    pub worktree: String,
    pub title: String,
    pub created_at: String,
    pub pending_title_session: String,
}

/// Mark a workbench as needing auto-title generation for the given CC session.
pub async fn set_pending_title_session(
    pool: &SqlitePool,
    id: i64,
    session_id: &str,
) -> Result<bool> {
    let result =
        sqlx::query("UPDATE workbenches SET pending_title_session = ?, rev = rev + 1 WHERE id = ?")
            .bind(session_id)
            .bind(id)
            .execute(pool)
            .await?;
    Ok(result.rows_affected() > 0)
}

/// Clear the pending auto-title flag after success or permanent failure.
pub async fn clear_pending_title_session(pool: &SqlitePool, id: i64) -> Result<bool> {
    let result = sqlx::query(
        "UPDATE workbenches SET pending_title_session = NULL, rev = rev + 1 WHERE id = ?",
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// List workbenches that have a pending auto-title request.
pub async fn list_pending_title(pool: &SqlitePool) -> Result<Vec<PendingTitleRow>> {
    let rows: Vec<PendingTitleRow> = sqlx::query_as(
        "SELECT id, worktree, title, created_at, pending_title_session \
         FROM workbenches \
         WHERE pending_title_session IS NOT NULL \
           AND archived_at IS NULL AND deleted_at IS NULL",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
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
