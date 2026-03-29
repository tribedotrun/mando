//! Task queries.

use std::{collections::HashMap, sync::OnceLock};

use anyhow::Result;
use sqlx::{query::Query, sqlite::SqliteArguments, Sqlite, SqlitePool};

use mando_types::task::{Task, TaskRouting};

use super::tasks_row::{RoutingRow, TaskRow};

type SqliteQuery<'q> = Query<'q, Sqlite, SqliteArguments<'q>>;

/// Fetch a single task by ID.
pub async fn find_by_id(pool: &SqlitePool, id: i64) -> Result<Option<Task>> {
    find_by_id_exec(pool, id).await
}

/// Fetch a single task by its Linear ID (e.g. "ENG-123").
pub async fn find_by_linear_id(pool: &SqlitePool, linear_id: &str) -> Result<Option<Task>> {
    let row: Option<TaskRow> = sqlx::query_as("SELECT * FROM tasks WHERE linear_id = ?")
        .bind(linear_id)
        .fetch_optional(pool)
        .await?;
    Ok(row.map(|r| r.into_task()))
}

/// Load all non-archived tasks.
pub async fn load_all(pool: &SqlitePool) -> Result<Vec<Task>> {
    let rows: Vec<TaskRow> = sqlx::query_as("SELECT * FROM tasks WHERE archived_at IS NULL")
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(|r| r.into_task()).collect())
}

/// Load all tasks including archived.
pub async fn load_all_with_archived(pool: &SqlitePool) -> Result<Vec<Task>> {
    let rows: Vec<TaskRow> = sqlx::query_as("SELECT * FROM tasks")
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(|r| r.into_task()).collect())
}

/// Load routing-level fields only (lighter query).
pub async fn routing(pool: &SqlitePool) -> Result<Vec<TaskRouting>> {
    let rows: Vec<RoutingRow> = sqlx::query_as(
        "SELECT id, title, status, project, worker, linear_id, resource
         FROM tasks WHERE archived_at IS NULL",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|r| r.into_routing()).collect())
}

// ── Column list constants ────────────────────────────────────────────────────

const INSERT_COLS: &str = "title, status, project, worker, linear_id, resource, context,
    original_prompt, created_at, worktree, branch, pr, worker_started_at,
    intervention_count, captain_review_trigger, session_ids,
    clarifier_questions, last_activity_at, plan, no_pr, worker_seq, reopen_seq,
    reopen_source, images, retry_count, escalation_report, source, archived_at";

const INSERT_PLACEHOLDERS: &str =
    "?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24,?25,?26,?27,?28";

const INSERT_WITH_ID_PLACEHOLDERS: &str =
    "?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23,?24,?25,?26,?27,?28,?29";

const UPDATE_SET: &str = "title=?1, status=?2, project=?3, worker=?4, linear_id=?5, resource=?6,
    context=?7, original_prompt=?8, created_at=?9, worktree=?10, branch=?11, pr=?12,
    worker_started_at=?13, intervention_count=?14, captain_review_trigger=?15,
    session_ids=?16, clarifier_questions=?17, last_activity_at=?18, plan=?19,
    no_pr=?20, worker_seq=?21, reopen_seq=?22, reopen_source=?23, images=?24, retry_count=?25,
    escalation_report=?26, source=?27, archived_at=?28";
fn insert_task_sql() -> &'static str {
    static SQL: OnceLock<String> = OnceLock::new();
    SQL.get_or_init(|| format!("INSERT INTO tasks ({INSERT_COLS}) VALUES ({INSERT_PLACEHOLDERS})"))
        .as_str()
}

fn insert_task_with_id_sql() -> &'static str {
    static SQL: OnceLock<String> = OnceLock::new();
    SQL.get_or_init(|| {
        format!("INSERT INTO tasks (id, {INSERT_COLS}) VALUES ({INSERT_WITH_ID_PLACEHOLDERS})")
    })
    .as_str()
}

fn update_task_sql() -> &'static str {
    static SQL: OnceLock<String> = OnceLock::new();
    SQL.get_or_init(|| format!("UPDATE tasks SET {UPDATE_SET} WHERE id=?29"))
        .as_str()
}

fn bind_task_write_fields<'q>(query: SqliteQuery<'q>, task: &'q Task) -> SqliteQuery<'q> {
    let trigger_str = task
        .captain_review_trigger
        .map(|trigger| trigger.as_str().to_string());

    query
        .bind(&task.title)
        .bind(task.status.as_str())
        .bind(&task.project)
        .bind(&task.worker)
        .bind(&task.linear_id)
        .bind(&task.resource)
        .bind(&task.context)
        .bind(&task.original_prompt)
        .bind(&task.created_at)
        .bind(&task.worktree)
        .bind(&task.branch)
        .bind(&task.pr)
        .bind(&task.worker_started_at)
        .bind(task.intervention_count)
        .bind(trigger_str)
        .bind(task.session_ids.to_json())
        .bind(&task.clarifier_questions)
        .bind(&task.last_activity_at)
        .bind(&task.plan)
        .bind(task.no_pr as i64)
        .bind(task.worker_seq)
        .bind(task.reopen_seq)
        .bind(&task.reopen_source)
        .bind(&task.images)
        .bind(task.retry_count)
        .bind(&task.escalation_report)
        .bind(&task.source)
        .bind(&task.archived_at)
}

/// Insert a new task (auto-ID).
pub async fn insert_task(pool: &SqlitePool, task: &Task) -> Result<i64> {
    let result = bind_task_write_fields(sqlx::query(insert_task_sql()), task)
        .execute(pool)
        .await?;
    Ok(result.last_insert_rowid())
}

/// Insert a task with an explicit ID.
pub async fn insert_task_with_id(pool: &SqlitePool, task: &Task) -> Result<()> {
    bind_task_write_fields(sqlx::query(insert_task_with_id_sql()).bind(task.id), task)
        .execute(pool)
        .await?;
    Ok(())
}
/// Update a full task row.
pub async fn update_task(pool: &SqlitePool, task: &Task) -> Result<bool> {
    update_task_exec(pool, task).await
}

/// Delete a task by ID.
pub async fn remove(pool: &SqlitePool, id: i64) -> Result<bool> {
    let result = sqlx::query("DELETE FROM tasks WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

/// Status counts for non-archived tasks.
pub async fn status_counts(pool: &SqlitePool) -> Result<HashMap<String, usize>> {
    let rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT status, COUNT(*) FROM tasks WHERE archived_at IS NULL GROUP BY status",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(|(s, c)| (s, c as usize)).collect())
}

/// Check if an active (non-terminal) task exists with the given source.
pub async fn has_active_with_source(pool: &SqlitePool, source: &str) -> Result<bool> {
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM tasks WHERE source = ? AND status NOT IN ('merged','completed-no-pr','canceled') AND archived_at IS NULL LIMIT 1)",
    )
    .bind(source)
    .fetch_one(pool)
    .await?;
    Ok(exists)
}

/// Count of active workers.
pub async fn active_worker_count(pool: &SqlitePool) -> Result<usize> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM tasks WHERE status='in-progress' AND worker IS NOT NULL AND archived_at IS NULL",
    )
    .fetch_one(pool)
    .await?;
    Ok(count as usize)
}

/// Replace all non-archived tasks atomically.
pub async fn replace_all(pool: &SqlitePool, tasks: &[Task]) -> Result<()> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM tasks WHERE archived_at IS NULL")
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

/// Archive terminal tasks older than `grace_secs`.
pub async fn archive_terminal(pool: &SqlitePool, grace_secs: u64) -> Result<usize> {
    let cutoff = time::OffsetDateTime::now_utc() - time::Duration::seconds(grace_secs as i64);
    let cutoff_str = cutoff
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap();
    let now_str = mando_types::now_rfc3339();

    let result = sqlx::query(
        "UPDATE tasks SET archived_at = ?
         WHERE archived_at IS NULL
           AND status IN ('merged', 'completed-no-pr', 'canceled')
           AND (COALESCE(last_activity_at, created_at) IS NULL
                OR datetime(COALESCE(last_activity_at, created_at)) <= datetime(?))",
    )
    .bind(&now_str)
    .bind(&cutoff_str)
    .execute(pool)
    .await?;

    let archived = result.rows_affected() as usize;
    if archived > 0 {
        tracing::info!(module = "task-store", archived, "terminal tasks archived");
    }
    Ok(archived)
}

/// Archive a task (set archived_at to now).
pub async fn archive_by_id(pool: &SqlitePool, id: i64) -> Result<bool> {
    let now = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default();
    let result = sqlx::query("UPDATE tasks SET archived_at = ? WHERE id = ?")
        .bind(&now)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

/// Un-archive a task (set archived_at back to NULL).
pub async fn unarchive(pool: &SqlitePool, id: i64) -> Result<bool> {
    let result = sqlx::query("UPDATE tasks SET archived_at = NULL WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
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
    let row: Option<TaskRow> = sqlx::query_as("SELECT * FROM tasks WHERE id = ?")
        .bind(id)
        .fetch_optional(exec)
        .await?;
    Ok(row.map(|r| r.into_task()))
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
