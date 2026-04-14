//! Task queries.

use std::{collections::HashMap, sync::OnceLock};

use anyhow::Result;
use sqlx::{query::Query, sqlite::SqliteArguments, Sqlite, SqlitePool};

use mando_types::task::{Task, TaskRouting};

use super::tasks_row::{RoutingRow, TaskRow};

// Re-export persist helpers so `queries::tasks::persist_*` paths keep working.
pub use super::tasks_persist::{
    persist_clarify_result, persist_clarify_start, persist_merge_spawn, persist_spawn,
    persist_status_transition,
};

type SqliteQuery<'q> = Query<'q, Sqlite, SqliteArguments<'q>>;

/// Explicit column list matching [`TaskRow`] fields — avoids `SELECT *` which
/// can break after `ALTER TABLE DROP COLUMN` due to sqlx type-inference on
/// removed column slots.
const SELECT_COLS: &str = "\
    t.id, t.title, t.status, t.project_id, p.name AS project, \
    t.worker, t.resource, t.context, t.original_prompt, \
    t.created_at, t.workbench_id, w.worktree, t.pr_number, t.worker_started_at, \
    t.intervention_count, t.captain_review_trigger, t.session_ids, t.last_activity_at, \
    t.plan, t.no_pr, t.worker_seq, t.reopen_seq, t.reopened_at, t.reopen_source, t.images, \
    t.review_fail_count, t.clarifier_fail_count, t.spawn_fail_count, t.merge_fail_count, \
    t.escalation_report, t.source, t.rev, p.github_repo";

fn select_tasks_sql() -> &'static str {
    static SQL: OnceLock<String> = OnceLock::new();
    SQL.get_or_init(|| {
        format!(
            "SELECT {SELECT_COLS} FROM tasks t \
             JOIN projects p ON p.id = t.project_id \
             LEFT JOIN workbenches w ON w.id = t.workbench_id"
        )
    })
}

/// Fetch a single task by ID.
pub async fn find_by_id(pool: &SqlitePool, id: i64) -> Result<Option<Task>> {
    find_by_id_exec(pool, id).await
}

/// Load all non-archived tasks (archive is on workbench, not task).
pub async fn load_all(pool: &SqlitePool) -> Result<Vec<Task>> {
    let sql = format!(
        "{} WHERE (t.workbench_id IS NULL OR w.archived_at IS NULL AND w.deleted_at IS NULL)",
        select_tasks_sql()
    );
    let rows: Vec<TaskRow> = sqlx::query_as(&sql).fetch_all(pool).await?;
    rows.into_iter().map(|r| r.into_task()).collect()
}

/// Load all tasks including archived.
pub async fn load_all_with_archived(pool: &SqlitePool) -> Result<Vec<Task>> {
    let rows: Vec<TaskRow> = sqlx::query_as(select_tasks_sql()).fetch_all(pool).await?;
    rows.into_iter().map(|r| r.into_task()).collect()
}

/// Load routing-level fields only (lighter query).
pub async fn routing(pool: &SqlitePool) -> Result<Vec<TaskRouting>> {
    let rows: Vec<RoutingRow> = sqlx::query_as(
        "SELECT t.id, t.title, t.status, t.project_id, p.name AS project, t.worker, t.resource
         FROM tasks t JOIN projects p ON p.id = t.project_id
         LEFT JOIN workbenches w ON w.id = t.workbench_id
         WHERE (t.workbench_id IS NULL OR w.archived_at IS NULL AND w.deleted_at IS NULL)",
    )
    .fetch_all(pool)
    .await?;
    rows.into_iter().map(|r| r.into_routing()).collect()
}

// ── Column list constants ────────────────────────────────────────────────────

/// Single source of truth for writable task columns.
/// Order must match the `.bind()` calls in [`bind_task_write_fields`].
const WRITE_COLS: &[&str] = &[
    "title",
    "status",
    "project_id",
    "worker",
    "resource",
    "context",
    "original_prompt",
    "created_at",
    "workbench_id",
    "pr_number",
    "worker_started_at",
    "intervention_count",
    "captain_review_trigger",
    "session_ids",
    "last_activity_at",
    "plan",
    "no_pr",
    "worker_seq",
    "reopen_seq",
    "reopened_at",
    "reopen_source",
    "images",
    "review_fail_count",
    "clarifier_fail_count",
    "spawn_fail_count",
    "merge_fail_count",
    "escalation_report",
    "source",
];

fn insert_task_sql() -> &'static str {
    static SQL: OnceLock<String> = OnceLock::new();
    SQL.get_or_init(|| {
        let cols = WRITE_COLS.join(", ");
        let placeholders = vec!["?"; WRITE_COLS.len()].join(", ");
        format!("INSERT INTO tasks ({cols}) VALUES ({placeholders})")
    })
}

fn insert_task_with_id_sql() -> &'static str {
    static SQL: OnceLock<String> = OnceLock::new();
    SQL.get_or_init(|| {
        let cols = WRITE_COLS.join(", ");
        let placeholders = vec!["?"; WRITE_COLS.len() + 1].join(", ");
        format!("INSERT INTO tasks (id, {cols}) VALUES ({placeholders})")
    })
}

/// Generate `col1=?, col2=?, ..., rev = rev + 1` SET clause from WRITE_COLS.
pub(crate) fn update_set_clause() -> String {
    let mut parts: Vec<String> = WRITE_COLS.iter().map(|c| format!("{c}=?")).collect();
    parts.push("rev = rev + 1".into());
    parts.join(", ")
}

fn update_task_sql() -> &'static str {
    static SQL: OnceLock<String> = OnceLock::new();
    SQL.get_or_init(|| format!("UPDATE tasks SET {} WHERE id=?", update_set_clause()))
}

pub(crate) fn bind_task_write_fields<'q>(
    query: SqliteQuery<'q>,
    task: &'q Task,
) -> SqliteQuery<'q> {
    let trigger_str = task
        .captain_review_trigger
        .map(|trigger| trigger.as_str().to_string());

    query
        .bind(&task.title)
        .bind(task.status.as_str())
        .bind(task.project_id)
        .bind(&task.worker)
        .bind(&task.resource)
        .bind(&task.context)
        .bind(&task.original_prompt)
        .bind(&task.created_at)
        .bind(task.workbench_id)
        .bind(task.pr_number)
        .bind(&task.worker_started_at)
        .bind(task.intervention_count)
        .bind(trigger_str)
        .bind(task.session_ids.to_json())
        .bind(&task.last_activity_at)
        .bind(&task.plan)
        .bind(task.no_pr as i64)
        .bind(task.worker_seq)
        .bind(task.reopen_seq)
        .bind(&task.reopened_at)
        .bind(&task.reopen_source)
        .bind(&task.images)
        .bind(task.review_fail_count)
        .bind(task.clarifier_fail_count)
        .bind(task.spawn_fail_count)
        .bind(task.merge_fail_count)
        .bind(&task.escalation_report)
        .bind(&task.source)
}

/// Insert a new task (auto-ID).
pub async fn insert_task(pool: &SqlitePool, task: &Task) -> Result<i64> {
    let result = bind_task_write_fields(sqlx::query(insert_task_sql()), task)
        .execute(pool)
        .await?;
    Ok(result.last_insert_rowid())
}

/// Update a full task row.
pub async fn update_task(pool: &SqlitePool, task: &Task) -> Result<bool> {
    update_task_exec(pool, task).await
}

/// Delete a task and its dependent rows (timeline_events, ask_history).
///
/// `task_rebase_state` cascades via FK; `cc_sessions` are cleaned up by
/// `task_cleanup::cleanup_task` before this function is called.
pub async fn remove(pool: &SqlitePool, id: i64) -> Result<bool> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM timeline_events WHERE task_id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM ask_history WHERE task_id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    let result = sqlx::query("DELETE FROM tasks WHERE id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(result.rows_affected() > 0)
}

/// Status counts for non-archived tasks.
pub async fn status_counts(pool: &SqlitePool) -> Result<HashMap<String, usize>> {
    let rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT t.status, COUNT(*) FROM tasks t \
         LEFT JOIN workbenches w ON w.id = t.workbench_id \
         WHERE (t.workbench_id IS NULL OR w.archived_at IS NULL AND w.deleted_at IS NULL) \
         GROUP BY t.status",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(s, c)| (s, c as usize)).collect())
}

/// Check if an active (non-terminal) task exists with the given source.
pub async fn has_active_with_source(pool: &SqlitePool, source: &str) -> Result<bool> {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM tasks t \
         LEFT JOIN workbenches w ON w.id = t.workbench_id \
         WHERE t.source = ? \
           AND t.status NOT IN ('merged','completed-no-pr','canceled') \
           AND (t.workbench_id IS NULL OR w.archived_at IS NULL AND w.deleted_at IS NULL) \
         LIMIT 1)",
    )
    .bind(source)
    .fetch_one(pool)
    .await?;
    Ok(exists)
}

/// Count of active workers.
pub async fn active_worker_count(pool: &SqlitePool) -> Result<usize> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM tasks t \
         LEFT JOIN workbenches w ON w.id = t.workbench_id \
         WHERE t.status='in-progress' AND t.worker IS NOT NULL \
           AND (t.workbench_id IS NULL OR w.archived_at IS NULL AND w.deleted_at IS NULL)",
    )
    .fetch_one(pool)
    .await?;
    Ok(count as usize)
}

/// Daily merge counts for the last N days (for the activity heatmap).
pub async fn daily_merge_counts(pool: &SqlitePool, days: u32) -> Result<Vec<(String, i64)>> {
    let rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT DATE(t.last_activity_at) AS day, COUNT(*) \
         FROM tasks t \
         WHERE t.status = 'merged' \
           AND t.last_activity_at >= datetime('now', '-' || ? || ' days') \
         GROUP BY day \
         ORDER BY day",
    )
    .bind(days)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Check whether a workbench has any active (non-finalized) tasks.
pub async fn has_active_for_workbench(pool: &SqlitePool, workbench_id: i64) -> Result<bool> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM tasks WHERE workbench_id = ? \
         AND status NOT IN ('merged','completed-no-pr','canceled')",
    )
    .bind(workbench_id)
    .fetch_one(pool)
    .await?;
    Ok(count > 0)
}

/// Replace all non-archived tasks atomically.
pub async fn replace_all(pool: &SqlitePool, tasks: &[Task]) -> Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query(
        "DELETE FROM tasks WHERE id IN (\
         SELECT t.id FROM tasks t \
         LEFT JOIN workbenches w ON w.id = t.workbench_id \
         WHERE t.workbench_id IS NULL OR w.archived_at IS NULL AND w.deleted_at IS NULL)",
    )
    .execute(&mut *tx)
    .await?;
    for task in tasks {
        if task.id > 0 {
            insert_task_with_id_tx(&mut tx, task).await?;
        } else {
            insert_task_tx(&mut tx, task).await?;
        }
    }
    tx.commit().await?;
    Ok(())
}

/// Atomically merge tick-changed items into the store.
pub async fn merge_changed_items(
    pool: &SqlitePool,
    pre_tick_snapshot: &HashMap<i64, serde_json::Value>,
    changed_items: &[Task],
    merge_fn: impl Fn(&serde_json::Value, &Task, &Task) -> Result<Task>,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    for changed in changed_items {
        if let Some(base_snapshot) = pre_tick_snapshot.get(&changed.id) {
            let current = find_by_id_exec(&mut *tx, changed.id).await?;
            let Some(current) = current else {
                tracing::warn!(
                    module = "task-store",
                    id = changed.id,
                    "skipping tick merge for deleted task"
                );
                continue;
            };
            // Terminal-status guard: if a human cancelled/merged the task
            // while the tick was in-flight, don't overwrite it.
            if current.status.is_finalized() {
                tracing::info!(
                    module = "task-store",
                    id = changed.id,
                    status = %current.status.as_str(),
                    "skipping tick merge for finalized task"
                );
                continue;
            }
            let merged = merge_fn(base_snapshot, changed, &current)?;
            update_task_exec(&mut *tx, &merged).await?;
            continue;
        }

        if changed.id > 0 {
            let exists = find_by_id_exec(&mut *tx, changed.id).await?.is_some();
            if exists {
                update_task_exec(&mut *tx, changed).await?;
            } else {
                insert_task_with_id_tx(&mut tx, changed).await?;
            }
        } else {
            insert_task_tx(&mut tx, changed).await?;
        }
    }
    tx.commit().await?;
    Ok(())
}

async fn insert_task_tx(tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>, task: &Task) -> Result<()> {
    bind_task_write_fields(sqlx::query(insert_task_sql()), task)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn insert_task_with_id_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    task: &Task,
) -> Result<()> {
    bind_task_write_fields(sqlx::query(insert_task_with_id_sql()).bind(task.id), task)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn find_by_id_exec<'e>(
    exec: impl sqlx::Executor<'e, Database = sqlx::Sqlite>,
    id: i64,
) -> Result<Option<Task>> {
    let sql = format!("{} WHERE t.id = ?", select_tasks_sql());
    let row: Option<TaskRow> = sqlx::query_as(&sql).bind(id).fetch_optional(exec).await?;
    row.map(|r| r.into_task()).transpose()
}

async fn update_task_exec<'e>(
    exec: impl sqlx::Executor<'e, Database = sqlx::Sqlite>,
    task: &Task,
) -> Result<bool> {
    let result = bind_task_write_fields(sqlx::query(update_task_sql()), task)
        .bind(task.id)
        .execute(exec)
        .await?;
    Ok(result.rows_affected() > 0)
}
