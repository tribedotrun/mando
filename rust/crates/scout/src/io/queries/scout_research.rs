//! Scout research run queries.

use anyhow::{Context, Result};
use global_db::lifecycle::{
    drain_record_only_outbox, record_transition, LifecycleEffect, LifecycleTransitionRecord,
};
use serde_json::json;
use sqlx::SqlitePool;

use crate::service::lifecycle::{apply_research_command, ResearchRunCommand};
use crate::{ResearchRunStatus, ScoutResearchRun};

async fn load_run_status_and_rev(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    id: i64,
) -> Result<Option<(String, i64)>> {
    sqlx::query_as::<_, (String, i64)>("SELECT status, rev FROM scout_research_runs WHERE id = ?")
        .bind(id)
        .fetch_optional(&mut **tx)
        .await
        .context("load scout research run status and rev")
}

async fn record_run_transition(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    id: i64,
    command: ResearchRunCommand,
    from_state: Option<&str>,
    to_state: &str,
    rev_before: i64,
    metadata: &serde_json::Value,
) -> Result<i64> {
    let aggregate_id = id.to_string();
    let effect_payload = json!({ "kind": "transition_recorded" });
    record_transition(
        tx,
        &LifecycleTransitionRecord {
            aggregate_type: "scout_research_run",
            aggregate_id: &aggregate_id,
            command: command.as_str(),
            from_state,
            to_state,
            actor: "scout",
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

/// Insert a new research run (status=running), return its ID.
pub async fn insert_run(pool: &SqlitePool, prompt: &str) -> Result<i64> {
    let now = global_types::now_rfc3339();
    let status = apply_research_command(None, ResearchRunCommand::Start)?;
    let mut tx = pool.begin().await?;
    let result = sqlx::query(
        "INSERT INTO scout_research_runs (research_prompt, status, created_at, rev) VALUES (?, ?, ?, 1)",
    )
    .bind(prompt)
    .bind(status.as_str())
    .bind(&now)
    .execute(&mut *tx)
    .await?;
    let id = result.last_insert_rowid();
    let metadata = json!({"id": id, "prompt": prompt, "to": status.as_str()});
    let transition_id = record_run_transition(
        &mut tx,
        id,
        ResearchRunCommand::Start,
        None,
        status.as_str(),
        0,
        &metadata,
    )
    .await?;
    tx.commit().await?;
    drain_record_only_outbox(pool, transition_id).await?;
    Ok(id)
}

/// Mark a research run as completed.
pub async fn complete_run(
    pool: &SqlitePool,
    id: i64,
    session_id: &str,
    added_count: i64,
) -> Result<()> {
    let now = global_types::now_rfc3339();
    let mut tx = pool.begin().await?;
    let Some((current_status, current_rev)) = load_run_status_and_rev(&mut tx, id).await? else {
        return Ok(());
    };
    let target = apply_research_command(
        Some(
            current_status
                .parse::<ResearchRunStatus>()
                .map_err(anyhow::Error::msg)?,
        ),
        ResearchRunCommand::Complete,
    )?;
    let result = sqlx::query(
        "UPDATE scout_research_runs
         SET status = ?, session_id = ?, added_count = ?, completed_at = ?, rev = rev + 1
         WHERE id = ? AND rev = ?",
    )
    .bind(target.as_str())
    .bind(session_id)
    .bind(added_count)
    .bind(&now)
    .bind(id)
    .bind(current_rev)
    .execute(&mut *tx)
    .await?;
    if result.rows_affected() == 0 {
        tx.rollback().await?;
        return Ok(());
    }
    let metadata = json!({"id": id, "from": current_status, "to": target.as_str(), "session_id": session_id, "added_count": added_count});
    let transition_id = record_run_transition(
        &mut tx,
        id,
        ResearchRunCommand::Complete,
        Some(current_status.as_str()),
        target.as_str(),
        current_rev,
        &metadata,
    )
    .await?;
    tx.commit().await?;
    drain_record_only_outbox(pool, transition_id).await?;
    Ok(())
}

/// Mark a research run as failed.
pub async fn fail_run(pool: &SqlitePool, id: i64, error: &str) -> Result<()> {
    let now = global_types::now_rfc3339();
    let mut tx = pool.begin().await?;
    let Some((current_status, current_rev)) = load_run_status_and_rev(&mut tx, id).await? else {
        return Ok(());
    };
    let target = apply_research_command(
        Some(
            current_status
                .parse::<ResearchRunStatus>()
                .map_err(anyhow::Error::msg)?,
        ),
        ResearchRunCommand::Fail,
    )?;
    let result = sqlx::query(
        "UPDATE scout_research_runs
         SET status = ?, error = ?, completed_at = ?, rev = rev + 1
         WHERE id = ? AND rev = ?",
    )
    .bind(target.as_str())
    .bind(error)
    .bind(&now)
    .bind(id)
    .bind(current_rev)
    .execute(&mut *tx)
    .await?;
    if result.rows_affected() == 0 {
        tx.rollback().await?;
        return Ok(());
    }
    let metadata = json!({"id": id, "from": current_status, "to": target.as_str(), "error": error});
    let transition_id = record_run_transition(
        &mut tx,
        id,
        ResearchRunCommand::Fail,
        Some(current_status.as_str()),
        target.as_str(),
        current_rev,
        &metadata,
    )
    .await?;
    tx.commit().await?;
    drain_record_only_outbox(pool, transition_id).await?;
    Ok(())
}

/// Get a research run by ID.
pub async fn get_run(pool: &SqlitePool, id: i64) -> Result<Option<ScoutResearchRun>> {
    let row: Option<RunRow> = sqlx::query_as(
        "SELECT id, research_prompt, status, error, session_id, added_count, created_at, completed_at, rev FROM scout_research_runs WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    row.map(RunRow::into_run).transpose()
}

/// Mark all runs stuck at `running` as failed (called on startup to
/// recover from daemon crashes that left orphan rows behind).
pub async fn reset_stale_running(pool: &SqlitePool) -> Result<u64> {
    let now = global_types::now_rfc3339();
    let target = apply_research_command(
        Some(ResearchRunStatus::Running),
        ResearchRunCommand::RecoverInterrupted,
    )?;
    let mut tx = pool.begin().await?;
    let mut transition_ids = Vec::new();
    let runs: Vec<(i64, i64)> =
        sqlx::query_as("SELECT id, rev FROM scout_research_runs WHERE status = 'running'")
            .fetch_all(&mut *tx)
            .await?;
    for (id, rev) in &runs {
        let result = sqlx::query(
            "UPDATE scout_research_runs
             SET status = ?, error = 'interrupted by daemon restart', completed_at = ?, rev = rev + 1
             WHERE id = ? AND rev = ?",
        )
        .bind(target.as_str())
        .bind(&now)
        .bind(id)
        .bind(rev)
        .execute(&mut *tx)
        .await?;
        if result.rows_affected() == 0 {
            continue;
        }
        let metadata = json!({
            "id": id,
            "from": "running",
            "to": target.as_str(),
            "reason": "daemon_restart"
        });
        let transition_id = record_run_transition(
            &mut tx,
            *id,
            ResearchRunCommand::RecoverInterrupted,
            Some("running"),
            target.as_str(),
            *rev,
            &metadata,
        )
        .await?;
        transition_ids.push(transition_id);
    }
    tx.commit().await?;
    let updated = transition_ids.len() as u64;
    for transition_id in transition_ids {
        drain_record_only_outbox(pool, transition_id).await?;
    }
    Ok(updated)
}

/// List recent research runs.
pub async fn list_runs(pool: &SqlitePool, limit: i64) -> Result<Vec<ScoutResearchRun>> {
    let rows: Vec<RunRow> = sqlx::query_as(
        "SELECT id, research_prompt, status, error, session_id, added_count, created_at, completed_at, rev FROM scout_research_runs ORDER BY id DESC LIMIT ?",
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;
    rows.into_iter().map(RunRow::into_run).collect()
}

#[derive(sqlx::FromRow)]
struct RunRow {
    id: i64,
    research_prompt: String,
    status: String,
    error: Option<String>,
    session_id: Option<String>,
    added_count: i64,
    created_at: String,
    completed_at: Option<String>,
    rev: i64,
}

impl RunRow {
    fn into_run(self) -> Result<ScoutResearchRun> {
        let status = self
            .status
            .parse::<ResearchRunStatus>()
            .map_err(|err: String| {
                anyhow::anyhow!("invalid scout research run status in database: {err}")
            })?;
        Ok(ScoutResearchRun {
            id: self.id,
            research_prompt: self.research_prompt,
            status,
            error: self.error,
            session_id: self.session_id,
            added_count: self.added_count,
            created_at: self.created_at,
            completed_at: self.completed_at,
            rev: self.rev,
        })
    }
}
