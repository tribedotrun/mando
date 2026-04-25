use anyhow::{bail, Context, Result};
use global_db::lifecycle::{
    drain_record_only_outbox, record_transition, LifecycleEffect, LifecycleTransitionRecord,
};
use serde_json::json;
use sqlx::SqlitePool;

use crate::service::lifecycle::{apply_item_command, ScoutItemCommand};
use crate::ScoutStatus;
async fn load_status_and_rev(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    id: i64,
) -> Result<Option<(String, i64)>> {
    let row = sqlx::query_as::<_, (Option<String>, i64)>(
        "SELECT status, rev FROM scout_items WHERE id = ?",
    )
    .bind(id)
    .fetch_optional(&mut **tx)
    .await
    .context("load scout item status and rev")?;
    Ok(row.map(|(status, rev)| (status.unwrap_or_else(|| "pending".to_string()), rev)))
}

async fn record_item_transition(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    id: i64,
    command: &str,
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
            aggregate_type: "scout_item",
            aggregate_id: &aggregate_id,
            command,
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

fn command_for_status(status: &str) -> Result<ScoutItemCommand> {
    match status {
        "pending" => Ok(ScoutItemCommand::MarkPending),
        "fetched" => Ok(ScoutItemCommand::MarkFetched),
        "processed" => Ok(ScoutItemCommand::MarkProcessed),
        "error" => Ok(ScoutItemCommand::MarkError),
        "saved" => Ok(ScoutItemCommand::Save),
        "archived" => Ok(ScoutItemCommand::Archive),
        other => bail!("unsupported scout lifecycle target status {other}"),
    }
}

pub async fn update_status(pool: &SqlitePool, id: i64, status: &str) -> Result<()> {
    let mut tx = pool.begin().await?;
    let Some((current_status, current_rev)) = load_status_and_rev(&mut tx, id).await? else {
        bail!("item #{id} not found");
    };
    let command = command_for_status(status)?;
    let current = super::parse_status(&current_status)?;
    let next = apply_item_command(current, command)?;
    if next == current {
        tx.rollback().await?;
        return Ok(());
    }
    let result =
        sqlx::query("UPDATE scout_items SET status = ?, rev = rev + 1 WHERE id = ? AND rev = ?")
            .bind(next.as_str())
            .bind(id)
            .bind(current_rev)
            .execute(&mut *tx)
            .await?;
    if result.rows_affected() == 0 {
        bail!("item #{id} changed concurrently");
    }
    let metadata = json!({"id": id, "from": current_status.clone(), "to": next.as_str()});
    let transition_id = record_item_transition(
        &mut tx,
        id,
        command.as_str(),
        Some(current_status.as_str()),
        next.as_str(),
        current_rev,
        &metadata,
    )
    .await?;
    tx.commit().await?;
    drain_record_only_outbox(pool, transition_id).await?;
    Ok(())
}

pub async fn update_status_if(
    pool: &SqlitePool,
    id: i64,
    status: &str,
    only_from: &[&str],
) -> Result<bool> {
    if only_from.is_empty() {
        bail!("only_from must not be empty");
    }
    let mut tx = pool.begin().await?;
    let Some((current_status, current_rev)) = load_status_and_rev(&mut tx, id).await? else {
        return Ok(false);
    };
    if !only_from
        .iter()
        .any(|candidate| *candidate == current_status)
    {
        tx.rollback().await?;
        return Ok(false);
    }
    let command = command_for_status(status)?;
    let current = super::parse_status(&current_status)?;
    let next = apply_item_command(current, command)?;
    if next == current {
        tx.rollback().await?;
        return Ok(true);
    }
    let result =
        sqlx::query("UPDATE scout_items SET status = ?, rev = rev + 1 WHERE id = ? AND rev = ?")
            .bind(next.as_str())
            .bind(id)
            .bind(current_rev)
            .execute(&mut *tx)
            .await?;
    if result.rows_affected() == 0 {
        tx.rollback().await?;
        return Ok(false);
    }
    let metadata = json!({"id": id, "from": current_status.clone(), "to": next.as_str()});
    let transition_id = record_item_transition(
        &mut tx,
        id,
        command.as_str(),
        Some(current_status.as_str()),
        next.as_str(),
        current_rev,
        &metadata,
    )
    .await?;
    tx.commit().await?;
    drain_record_only_outbox(pool, transition_id).await?;
    Ok(true)
}

#[allow(clippy::too_many_arguments)]
pub async fn update_processed(
    pool: &SqlitePool,
    id: i64,
    title: &str,
    relevance: i64,
    quality: i64,
    source_name: Option<&str>,
    date_published: Option<&str>,
    summary: &str,
    article: &str,
) -> Result<bool> {
    let now = global_types::now_rfc3339();
    let mut tx = pool.begin().await?;
    let Some((current_status, current_rev)) = load_status_and_rev(&mut tx, id).await? else {
        return Ok(false);
    };
    let current = super::parse_status(&current_status)?;
    let next = match apply_item_command(current, ScoutItemCommand::MarkProcessed) {
        Ok(next) => next,
        Err(_) => {
            tx.rollback().await?;
            return Ok(false);
        }
    };
    let result = sqlx::query(
        "UPDATE scout_items
         SET title = ?, relevance = ?, quality = ?,
             source_name = ?, status = ?, date_processed = ?,
             date_published = ?, summary = ?, article = ?, rev = rev + 1
         WHERE id = ? AND rev = ?",
    )
    .bind(title)
    .bind(relevance)
    .bind(quality)
    .bind(source_name)
    .bind(next.as_str())
    .bind(&now)
    .bind(date_published)
    .bind(summary)
    .bind(article)
    .bind(id)
    .bind(current_rev)
    .execute(&mut *tx)
    .await?;
    if result.rows_affected() == 0 {
        tx.rollback().await?;
        return Ok(false);
    }
    let metadata = json!({
        "id": id,
        "from": current_status.clone(),
        "to": next.as_str(),
        "title": title,
        "relevance": relevance,
        "quality": quality,
    });
    let transition_id = record_item_transition(
        &mut tx,
        id,
        ScoutItemCommand::MarkProcessed.as_str(),
        Some(current_status.as_str()),
        next.as_str(),
        current_rev,
        &metadata,
    )
    .await?;
    tx.commit().await?;
    drain_record_only_outbox(pool, transition_id).await?;
    Ok(true)
}

pub async fn increment_error_count(pool: &SqlitePool, id: i64) -> Result<()> {
    let mut tx = pool.begin().await?;
    let Some((current_status, current_rev)) = load_status_and_rev(&mut tx, id).await? else {
        bail!("item #{id} not found");
    };
    let current = super::parse_status(&current_status)?;
    let next = apply_item_command(current, ScoutItemCommand::MarkError)?;
    let result = sqlx::query(
        "UPDATE scout_items
         SET error_count = COALESCE(error_count, 0) + 1, status = ?, rev = rev + 1
         WHERE id = ? AND rev = ?",
    )
    .bind(next.as_str())
    .bind(id)
    .bind(current_rev)
    .execute(&mut *tx)
    .await?;
    if result.rows_affected() == 0 {
        bail!("item #{id} changed concurrently");
    }
    let metadata = json!({"id": id, "from": current_status.clone(), "to": next.as_str()});
    let transition_id = record_item_transition(
        &mut tx,
        id,
        ScoutItemCommand::MarkError.as_str(),
        Some(current_status.as_str()),
        next.as_str(),
        current_rev,
        &metadata,
    )
    .await?;
    tx.commit().await?;
    drain_record_only_outbox(pool, transition_id).await?;
    Ok(())
}

pub async fn increment_error_count_if_status(
    pool: &SqlitePool,
    id: i64,
    allowed_statuses: &[ScoutStatus],
) -> Result<bool> {
    let mut tx = pool.begin().await?;
    let Some((current_status, current_rev)) = load_status_and_rev(&mut tx, id).await? else {
        bail!("item #{id} not found");
    };
    let current = super::parse_status(&current_status)?;
    if !allowed_statuses.contains(&current) {
        tx.rollback().await?;
        return Ok(false);
    }
    let next = apply_item_command(current, ScoutItemCommand::MarkError)?;
    let result = sqlx::query(
        "UPDATE scout_items
         SET error_count = COALESCE(error_count, 0) + 1, status = ?, rev = rev + 1
         WHERE id = ? AND rev = ?",
    )
    .bind(next.as_str())
    .bind(id)
    .bind(current_rev)
    .execute(&mut *tx)
    .await?;
    if result.rows_affected() == 0 {
        tx.rollback().await?;
        return Ok(false);
    }
    let metadata = json!({"id": id, "from": current_status.clone(), "to": next.as_str()});
    let transition_id = record_item_transition(
        &mut tx,
        id,
        ScoutItemCommand::MarkError.as_str(),
        Some(current_status.as_str()),
        next.as_str(),
        current_rev,
        &metadata,
    )
    .await?;
    tx.commit().await?;
    drain_record_only_outbox(pool, transition_id).await?;
    Ok(true)
}

pub async fn reset_error_state(pool: &SqlitePool, id: i64) -> Result<()> {
    let mut tx = pool.begin().await?;
    let Some((current_status, current_rev)) = load_status_and_rev(&mut tx, id).await? else {
        bail!("item #{id} not found");
    };
    let current = super::parse_status(&current_status)?;
    let next = apply_item_command(current, ScoutItemCommand::MarkPending)?;
    let result = sqlx::query(
        "UPDATE scout_items
         SET error_count = 0, status = ?, rev = rev + 1
         WHERE id = ? AND rev = ?",
    )
    .bind(next.as_str())
    .bind(id)
    .bind(current_rev)
    .execute(&mut *tx)
    .await?;
    if result.rows_affected() == 0 {
        bail!("item #{id} changed concurrently");
    }
    let metadata = json!({"id": id, "from": current_status.clone(), "to": next.as_str()});
    let transition_id = record_item_transition(
        &mut tx,
        id,
        ScoutItemCommand::MarkPending.as_str(),
        Some(current_status.as_str()),
        next.as_str(),
        current_rev,
        &metadata,
    )
    .await?;
    tx.commit().await?;
    drain_record_only_outbox(pool, transition_id).await?;
    Ok(())
}

pub async fn reset_stale_fetched(pool: &SqlitePool) -> Result<u64> {
    let mut tx = pool.begin().await?;
    let items: Vec<(i64, i64)> =
        sqlx::query_as("SELECT id, rev FROM scout_items WHERE status = 'fetched'")
            .fetch_all(&mut *tx)
            .await?;
    let mut transition_ids = Vec::new();
    for (id, rev) in &items {
        let result = sqlx::query(
            "UPDATE scout_items SET status = ?, rev = rev + 1 WHERE id = ? AND rev = ?",
        )
        .bind("pending")
        .bind(id)
        .bind(rev)
        .execute(&mut *tx)
        .await?;
        if result.rows_affected() == 0 {
            continue;
        }
        let metadata = json!({"id": id, "from": "fetched", "to": "pending"});
        let transition_id = record_item_transition(
            &mut tx,
            *id,
            ScoutItemCommand::MarkPending.as_str(),
            Some("fetched"),
            "pending",
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
