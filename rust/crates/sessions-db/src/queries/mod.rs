//! Unified session queries — all CC sessions in one table.

use anyhow::{Context, Result};
use global_db::lifecycle::{
    drain_record_only_outbox, record_transition, LifecycleEffect, LifecycleTransitionRecord,
};
use serde_json::json;
use sqlx::SqlitePool;

use global_types::SessionStatus;

use crate::{CallerGroup, SessionCaller};

mod read;

pub use read::{
    category_counts, delete_sessions_for_task, find_session_id_by_worker_name, get_credential_id,
    is_session_running, list_running_sessions, list_running_sessions_for_task, list_sessions,
    list_sessions_for_scout_item, list_sessions_for_task, list_sessions_missing_cost,
    session_by_id, session_cwd, total_session_cost,
};

/// Column list for SessionRow queries — single source of truth.
const SELECT_COLS: &str = "\
    session_id, created_at, caller, cwd, model, status, \
    cost_usd, duration_ms, resumed, turn_count, \
    task_id, scout_item_id, worker_name, resumed_at, credential_id, \
    error, api_error_status, rev";

pub(crate) fn select_sessions_sql() -> &'static str {
    static SQL: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    SQL.get_or_init(|| format!("SELECT {SELECT_COLS} FROM cc_sessions"))
}

async fn load_session_status_and_rev(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    session_id: &str,
) -> Result<Option<(String, i64)>> {
    sqlx::query_as::<_, (String, i64)>("SELECT status, rev FROM cc_sessions WHERE session_id = ?")
        .bind(session_id)
        .fetch_optional(&mut **tx)
        .await
        .context("load session status and rev")
}

async fn record_session_transition(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    session_id: &str,
    command: crate::lifecycle::SessionLifecycleCommand,
    from_state: Option<&str>,
    to_state: &str,
    rev_before: i64,
    metadata: &serde_json::Value,
) -> Result<i64> {
    let effect_payload = json!({ "kind": "transition_recorded" });
    record_transition(
        tx,
        &LifecycleTransitionRecord {
            aggregate_type: "cc_session",
            aggregate_id: session_id,
            command: command.as_str(),
            from_state,
            to_state,
            actor: "sessions",
            cause: None,
            metadata,
            rev_before,
            rev_after: rev_before + 1,
            idempotency_key: None,
        },
        &[LifecycleEffect {
            effect_kind: "lifecycle.transition.recorded",
            payload: &effect_payload,
        }],
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn update_existing_session_row(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    input: &SessionUpsert<'_>,
    next_status: &str,
    expected_rev: i64,
) -> Result<u64> {
    Ok(sqlx::query(
        "UPDATE cc_sessions SET
            created_at = CASE WHEN ?1 != '' THEN ?1 ELSE created_at END,
            caller = CASE WHEN ?2 != '' THEN ?2 ELSE caller END,
            cwd = CASE WHEN ?3 != '' THEN ?3 ELSE cwd END,
            model = CASE WHEN ?4 != '' THEN ?4 ELSE model END,
            status = ?5,
            cost_usd = CASE
                WHEN ?6 IS NOT NULL AND cost_usd IS NOT NULL THEN cost_usd + ?6
                WHEN ?6 IS NOT NULL THEN ?6
                ELSE cost_usd
            END,
            duration_ms = CASE
                WHEN ?7 IS NOT NULL AND duration_ms IS NOT NULL THEN duration_ms + ?7
                WHEN ?7 IS NOT NULL THEN ?7
                ELSE duration_ms
            END,
            resumed = MAX(resumed, ?8),
            turn_count = CASE
                WHEN ?6 IS NOT NULL THEN turn_count + 1
                ELSE turn_count
            END,
            task_id = COALESCE(?9, task_id),
            scout_item_id = COALESCE(?10, scout_item_id),
            worker_name = COALESCE(?11, worker_name),
            resumed_at = CASE WHEN ?12 IS NOT NULL THEN ?12 ELSE resumed_at END,
            credential_id = COALESCE(?13, credential_id),
            error = COALESCE(?14, error),
            api_error_status = COALESCE(?15, api_error_status),
            rev = CASE WHEN ?5 != status THEN rev + 1 ELSE rev END
         WHERE session_id = ?16 AND rev = ?17",
    )
    .bind(input.created_at)
    .bind(input.caller)
    .bind(input.cwd)
    .bind(input.model)
    .bind(next_status)
    .bind(input.cost_usd)
    .bind(input.duration_ms)
    .bind(input.resumed as i64)
    .bind(input.task_id)
    .bind(input.scout_item_id)
    .bind(input.worker_name)
    .bind(input.resumed_at)
    .bind(input.credential_id)
    .bind(input.error)
    .bind(input.api_error_status)
    .bind(input.session_id)
    .bind(expected_rev)
    .execute(&mut **tx)
    .await?
    .rows_affected())
}

async fn update_session_status_row(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    session_id: &str,
    status: SessionStatus,
    expected_rev: i64,
) -> Result<u64> {
    Ok(sqlx::query(
        "UPDATE cc_sessions SET status = ?, rev = rev + 1 WHERE session_id = ? AND rev = ?",
    )
    .bind(status.as_str())
    .bind(session_id)
    .bind(expected_rev)
    .execute(&mut **tx)
    .await?
    .rows_affected())
}

async fn update_session_status_with_cost_row(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    session_id: &str,
    status: SessionStatus,
    cost_usd: Option<f64>,
    duration_ms: Option<i64>,
    num_turns: Option<i64>,
    expected_rev: i64,
) -> Result<u64> {
    Ok(sqlx::query(
        "UPDATE cc_sessions SET
            status = ?1,
            cost_usd = CASE
                WHEN ?2 IS NOT NULL AND cost_usd IS NOT NULL THEN cost_usd + ?2
                WHEN ?2 IS NOT NULL THEN ?2
                ELSE cost_usd
            END,
            duration_ms = CASE
                WHEN ?3 IS NOT NULL AND duration_ms IS NOT NULL THEN duration_ms + ?3
                WHEN ?3 IS NOT NULL THEN ?3
                ELSE duration_ms
            END,
            turn_count = CASE
                WHEN ?4 IS NOT NULL THEN turn_count + ?4
                ELSE turn_count
            END,
            rev = CASE WHEN ?1 != status THEN rev + 1 ELSE rev END
         WHERE session_id = ?5 AND rev = ?6",
    )
    .bind(status.as_str())
    .bind(cost_usd)
    .bind(duration_ms)
    .bind(num_turns)
    .bind(session_id)
    .bind(expected_rev)
    .execute(&mut **tx)
    .await?
    .rows_affected())
}

/// A session row from the unified sessions table.
#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct SessionRow {
    pub session_id: String,
    pub created_at: String,
    pub caller: String,
    pub cwd: String,
    pub model: String,
    pub status: String,
    pub cost_usd: Option<f64>,
    pub duration_ms: Option<i64>,
    pub resumed: i64,
    pub turn_count: i64,
    pub task_id: Option<i64>,
    pub scout_item_id: Option<i64>,
    pub worker_name: Option<String>,
    pub resumed_at: Option<String>,
    pub credential_id: Option<i64>,
    pub error: Option<String>,
    pub api_error_status: Option<i64>,
}

impl SessionRow {
    /// Parse the caller string into the enum.
    pub fn parsed_caller(&self) -> Option<SessionCaller> {
        SessionCaller::parse(&self.caller)
    }

    /// Get the display group for this session.
    pub fn group(&self) -> Option<CallerGroup> {
        self.parsed_caller().map(|c| c.group())
    }
}

/// Input for upserting a session.
pub struct SessionUpsert<'a> {
    pub session_id: &'a str,
    pub created_at: &'a str,
    pub caller: &'a str,
    pub cwd: &'a str,
    pub model: &'a str,
    pub status: SessionStatus,
    pub cost_usd: Option<f64>,
    pub duration_ms: Option<i64>,
    pub resumed: bool,
    pub task_id: Option<i64>,
    pub scout_item_id: Option<i64>,
    pub worker_name: Option<&'a str>,
    pub resumed_at: Option<&'a str>,
    pub credential_id: Option<i64>,
    pub error: Option<&'a str>,
    pub api_error_status: Option<i64>,
}

/// Upsert a session with cumulative cost tracking.
/// On conflict: cost and duration are ADDED (not replaced), turn_count increments,
/// resumed latches to true, other fields use "non-empty wins" logic.
pub async fn upsert_session(pool: &SqlitePool, input: &SessionUpsert<'_>) -> Result<()> {
    let mut tx = pool.begin().await?;
    let mut current = load_session_status_and_rev(&mut tx, input.session_id).await?;
    let next_status = input.status.as_str();
    let metadata = json!({
        "session_id": input.session_id,
        "caller": input.caller,
        "task_id": input.task_id,
        "scout_item_id": input.scout_item_id,
        "resumed": input.resumed,
    });

    if current.is_none() {
        let inserted = sqlx::query(
            "INSERT INTO cc_sessions (session_id, created_at, caller, cwd, model, status,
                cost_usd, duration_ms, resumed, turn_count, task_id, scout_item_id, worker_name,
                resumed_at, credential_id, error, api_error_status, rev)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 0, ?10, ?11, ?12, ?13, ?14, ?15, ?16, 1)
             ON CONFLICT(session_id) DO NOTHING",
        )
        .bind(input.session_id)
        .bind(input.created_at)
        .bind(input.caller)
        .bind(input.cwd)
        .bind(input.model)
        .bind(next_status)
        .bind(input.cost_usd)
        .bind(input.duration_ms)
        .bind(input.resumed as i64)
        .bind(input.task_id)
        .bind(input.scout_item_id)
        .bind(input.worker_name)
        .bind(input.resumed_at)
        .bind(input.credential_id)
        .bind(input.error)
        .bind(input.api_error_status)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        if inserted > 0 {
            let command = crate::lifecycle::infer_command(None, input.status, input.resumed)?;
            let transition_id = record_session_transition(
                &mut tx,
                input.session_id,
                command,
                None,
                next_status,
                0,
                &metadata,
            )
            .await?;
            tx.commit().await?;
            drain_record_only_outbox(pool, transition_id).await?;
            return Ok(());
        }
        current = load_session_status_and_rev(&mut tx, input.session_id).await?;
    }

    if let Some((current_status, current_rev)) = current {
        let rows_affected =
            update_existing_session_row(&mut tx, input, next_status, current_rev).await?;
        if rows_affected == 0 {
            tx.rollback().await?;
            return Ok(());
        }
        if current_status != next_status {
            let current_parsed = current_status.parse::<SessionStatus>()?;
            let command =
                crate::lifecycle::infer_command(Some(current_parsed), input.status, input.resumed)?;
            let transition_id = record_session_transition(
                &mut tx,
                input.session_id,
                command,
                Some(current_status.as_str()),
                next_status,
                current_rev,
                &metadata,
            )
            .await?;
            tx.commit().await?;
            drain_record_only_outbox(pool, transition_id).await?;
        } else {
            tx.commit().await?;
        }
    }
    Ok(())
}

/// Update only the status of a session (targeted update for reconciliation).
pub async fn update_session_status(
    pool: &SqlitePool,
    session_id: &str,
    status: SessionStatus,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    let Some((current_status, current_rev)) =
        load_session_status_and_rev(&mut tx, session_id).await?
    else {
        return Ok(());
    };
    if current_status == status.as_str() {
        tx.rollback().await?;
        return Ok(());
    }
    let rows_affected = update_session_status_row(&mut tx, session_id, status, current_rev).await?;
    if rows_affected == 0 {
        tx.rollback().await?;
        return Ok(());
    }
    let command = crate::lifecycle::infer_command(
        Some(current_status.parse::<SessionStatus>()?),
        status,
        false,
    )?;
    let metadata = json!({"session_id": session_id, "from": current_status, "to": status.as_str()});
    let transition_id = record_session_transition(
        &mut tx,
        session_id,
        command,
        Some(current_status.as_str()),
        status.as_str(),
        current_rev,
        &metadata,
    )
    .await?;
    tx.commit().await?;
    drain_record_only_outbox(pool, transition_id).await?;
    Ok(())
}

/// Update status and accumulate cost/duration from stream data.
///
/// Cost and duration use ADD semantics to accumulate across resume cycles.
/// Callers must guard against double-calling for the same segment (e.g.
/// `terminate_session` checks `is_session_running` before calling).
pub async fn update_session_status_with_cost(
    pool: &SqlitePool,
    session_id: &str,
    status: SessionStatus,
    cost_usd: Option<f64>,
    duration_ms: Option<i64>,
    num_turns: Option<i64>,
) -> Result<()> {
    let mut tx = pool.begin().await?;
    let Some((current_status, current_rev)) =
        load_session_status_and_rev(&mut tx, session_id).await?
    else {
        return Ok(());
    };
    let rows_affected = update_session_status_with_cost_row(
        &mut tx,
        session_id,
        status,
        cost_usd,
        duration_ms,
        num_turns,
        current_rev,
    )
    .await?;
    if rows_affected == 0 {
        tx.rollback().await?;
        return Ok(());
    }
    if current_status != status.as_str() {
        let command = crate::lifecycle::infer_command(
            Some(current_status.parse::<SessionStatus>()?),
            status,
            false,
        )?;
        let metadata = json!({
            "session_id": session_id,
            "from": current_status.clone(),
            "to": status.as_str(),
            "cost_usd": cost_usd,
            "duration_ms": duration_ms,
            "num_turns": num_turns,
        });
        let transition_id = record_session_transition(
            &mut tx,
            session_id,
            command,
            Some(current_status.as_str()),
            status.as_str(),
            current_rev,
            &metadata,
        )
        .await?;
        tx.commit().await?;
        drain_record_only_outbox(pool, transition_id).await?;
    } else {
        tx.commit().await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use global_db::Db;

    async fn test_pool() -> SqlitePool {
        let db = Db::open_in_memory().await.unwrap();
        db.pool().clone()
    }

    #[tokio::test]
    async fn upsert_and_list() {
        let pool = test_pool().await;
        upsert_session(
            &pool,
            &SessionUpsert {
                session_id: "s1",
                created_at: "2026-03-26T00:00:00Z",
                caller: "worker",
                cwd: "/tmp",
                model: "opus",
                status: SessionStatus::Stopped,
                cost_usd: Some(1.5),
                duration_ms: Some(30000),
                resumed: false,
                task_id: Some(1),
                scout_item_id: None,
                worker_name: Some("main-v1"),
                resumed_at: None,
                credential_id: None,
                error: None,
                api_error_status: None,
            },
        )
        .await
        .unwrap();

        let (rows, total) = list_sessions(&pool, 1, 50, None, None).await.unwrap();
        assert_eq!(total, 1);
        assert_eq!(rows[0].session_id, "s1");
        assert_eq!(rows[0].cost_usd, Some(1.5));
        assert_eq!(rows[0].turn_count, 0);
    }

    #[tokio::test]
    async fn cumulative_cost_on_upsert() {
        let pool = test_pool().await;
        // First turn
        upsert_session(
            &pool,
            &SessionUpsert {
                session_id: "s1",
                created_at: "2026-03-26T00:00:00Z",
                caller: "worker",
                cwd: "/tmp",
                model: "opus",
                status: SessionStatus::Running,
                cost_usd: Some(1.0),
                duration_ms: Some(10000),
                resumed: false,
                task_id: Some(1),
                scout_item_id: None,
                worker_name: None,
                resumed_at: None,
                credential_id: None,
                error: None,
                api_error_status: None,
            },
        )
        .await
        .unwrap();

        // Second turn (resume)
        upsert_session(
            &pool,
            &SessionUpsert {
                session_id: "s1",
                created_at: "",
                caller: "worker",
                cwd: "",
                model: "",
                status: SessionStatus::Stopped,
                cost_usd: Some(0.5),
                duration_ms: Some(5000),
                resumed: true,
                task_id: None,
                scout_item_id: None,
                worker_name: None,
                resumed_at: None,
                credential_id: None,
                error: None,
                api_error_status: None,
            },
        )
        .await
        .unwrap();

        let (rows, _) = list_sessions(&pool, 1, 50, None, None).await.unwrap();
        assert_eq!(rows[0].cost_usd, Some(1.5)); // 1.0 + 0.5
        assert_eq!(rows[0].duration_ms, Some(15000)); // 10000 + 5000
        assert_eq!(rows[0].turn_count, 1);
        assert_eq!(rows[0].resumed, 1);
    }
}
