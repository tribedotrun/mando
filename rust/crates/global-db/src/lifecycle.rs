use anyhow::{Context, Result};
use serde_json::Value;
use sqlx::{Sqlite, SqlitePool, Transaction};

#[derive(Debug, Clone, Copy)]
pub struct LifecycleEffect<'a> {
    pub effect_kind: &'a str,
    pub payload: &'a Value,
}

#[derive(Debug, Clone, Copy)]
pub struct LifecycleTransitionRecord<'a> {
    pub aggregate_type: &'a str,
    pub aggregate_id: &'a str,
    pub command: &'a str,
    pub from_state: Option<&'a str>,
    pub to_state: &'a str,
    pub actor: &'a str,
    pub cause: Option<&'a str>,
    pub metadata: &'a Value,
    pub rev_before: i64,
    pub rev_after: i64,
    pub idempotency_key: Option<&'a str>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct LifecycleOutboxRow {
    pub id: i64,
    pub transition_id: i64,
    pub aggregate_type: String,
    pub aggregate_id: String,
    pub effect_kind: String,
    pub payload: String,
}

pub async fn record_transition(
    tx: &mut Transaction<'_, Sqlite>,
    record: &LifecycleTransitionRecord<'_>,
    effects: &[LifecycleEffect<'_>],
) -> Result<i64> {
    let now = global_types::now_rfc3339();
    let metadata = serde_json::to_string(record.metadata)?;
    let result = sqlx::query(
        "INSERT INTO lifecycle_transitions (
            aggregate_type, aggregate_id, command, from_state, to_state,
            actor, cause, metadata, rev_before, rev_after, idempotency_key, occurred_at
         ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(record.aggregate_type)
    .bind(record.aggregate_id)
    .bind(record.command)
    .bind(record.from_state)
    .bind(record.to_state)
    .bind(record.actor)
    .bind(record.cause)
    .bind(metadata)
    .bind(record.rev_before)
    .bind(record.rev_after)
    .bind(record.idempotency_key)
    .bind(&now)
    .execute(&mut **tx)
    .await
    .with_context(|| {
        format!(
            "insert lifecycle transition {}:{} {}",
            record.aggregate_type, record.aggregate_id, record.command
        )
    })?;
    let transition_id = result.last_insert_rowid();

    for effect in effects {
        let payload = serde_json::to_string(effect.payload)?;
        sqlx::query(
            "INSERT INTO lifecycle_outbox (
                transition_id, aggregate_type, aggregate_id, effect_kind, payload, created_at
             ) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(transition_id)
        .bind(record.aggregate_type)
        .bind(record.aggregate_id)
        .bind(effect.effect_kind)
        .bind(payload)
        .bind(&now)
        .execute(&mut **tx)
        .await
        .with_context(|| {
            format!(
                "insert lifecycle outbox effect {}:{} {}",
                record.aggregate_type, record.aggregate_id, effect.effect_kind
            )
        })?;
    }

    Ok(transition_id)
}

pub async fn pending_outbox_for_transition(
    pool: &SqlitePool,
    transition_id: i64,
) -> Result<Vec<LifecycleOutboxRow>> {
    sqlx::query_as(
        "SELECT id, transition_id, aggregate_type, aggregate_id, effect_kind, payload
         FROM lifecycle_outbox
         WHERE transition_id = ? AND processed_at IS NULL
         ORDER BY id ASC",
    )
    .bind(transition_id)
    .fetch_all(pool)
    .await
    .context("load lifecycle outbox rows")
}

pub async fn pending_outbox_rows(pool: &SqlitePool) -> Result<Vec<LifecycleOutboxRow>> {
    sqlx::query_as(
        "SELECT id, transition_id, aggregate_type, aggregate_id, effect_kind, payload
         FROM lifecycle_outbox
         WHERE processed_at IS NULL
         ORDER BY id ASC",
    )
    .fetch_all(pool)
    .await
    .context("load pending lifecycle outbox rows")
}

pub async fn mark_outbox_processed(pool: &SqlitePool, outbox_id: i64) -> Result<()> {
    sqlx::query(
        "UPDATE lifecycle_outbox
         SET processed_at = ?, attempts = attempts + 1, last_error = NULL
         WHERE id = ?",
    )
    .bind(global_types::now_rfc3339())
    .bind(outbox_id)
    .execute(pool)
    .await
    .context("mark lifecycle outbox processed")?;
    Ok(())
}

pub async fn mark_outbox_failed(pool: &SqlitePool, outbox_id: i64, error: &str) -> Result<()> {
    sqlx::query(
        "UPDATE lifecycle_outbox
         SET attempts = attempts + 1, last_error = ?
         WHERE id = ?",
    )
    .bind(error)
    .bind(outbox_id)
    .execute(pool)
    .await
    .context("mark lifecycle outbox failed")?;
    Ok(())
}

pub async fn drain_record_only_outbox(pool: &SqlitePool, transition_id: i64) -> Result<()> {
    let rows = pending_outbox_for_transition(pool, transition_id).await?;
    for row in rows {
        match row.effect_kind.as_str() {
            "lifecycle.transition.recorded" => {
                mark_outbox_processed(pool, row.id).await?;
                tracing::info!(
                    module = "global-db-lifecycle", transition_id = row.transition_id,
                    outbox_id = row.id,
                    effect_kind = %row.effect_kind,
                    aggregate_type = %row.aggregate_type,
                    aggregate_id = %row.aggregate_id,
                    "lifecycle outbox effect processed"
                );
            }
            other => {
                mark_outbox_failed(pool, row.id, &format!("unsupported effect kind: {other}"))
                    .await?;
                anyhow::bail!(
                    "unsupported lifecycle outbox effect kind {other} for transition {}",
                    row.transition_id
                );
            }
        }
    }
    Ok(())
}
