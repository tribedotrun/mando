//! CRUD and load-balancing queries for the credentials table.
//!
//! Credentials are setup tokens for additional Claude Code accounts.
//! When no credentials exist, workers use the host's ambient Claude Code login.
//!
//! Public types (`CredentialRow`, `CredentialInfo`, etc.) and the
//! `CredentialRow::to_info` mapper live in `credential_types.rs` and are
//! re-exported below — split out to keep this file under the file-length
//! budget for query code.

use std::collections::HashMap;

use anyhow::Result;
use sqlx::SqlitePool;

use crate::io::usage_probe::UsageSnapshot;

pub use crate::io::credential_types::{CredentialInfo, CredentialRow, CredentialWindowInfo};

/// Get labels for a list of credential IDs.
pub async fn labels_by_ids(pool: &SqlitePool, ids: &[i64]) -> Result<HashMap<i64, String>> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }
    let placeholders: Vec<String> = ids.iter().map(|_| "?".to_string()).collect();
    let sql = format!(
        "SELECT id, label FROM credentials WHERE id IN ({})",
        placeholders.join(",")
    );
    let mut query = sqlx::query_as::<_, (i64, String)>(&sql);
    for id in ids {
        query = query.bind(id);
    }
    let rows = query.fetch_all(pool).await?;
    Ok(rows.into_iter().collect())
}

/// Check if any Claude credentials are configured. The provider predicate is
/// kept for compatibility with databases that were migrated while Codex
/// account credentials existed.
pub async fn has_any(pool: &SqlitePool) -> Result<bool> {
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM credentials WHERE provider = 'claude'")
            .fetch_one(pool)
            .await?;
    Ok(count > 0)
}

/// List all Claude credentials (full rows including tokens).
pub async fn list_all(pool: &SqlitePool) -> Result<Vec<CredentialRow>> {
    let rows: Vec<CredentialRow> =
        sqlx::query_as("SELECT * FROM credentials WHERE provider = 'claude' ORDER BY label")
            .fetch_all(pool)
            .await?;
    Ok(rows)
}

/// Fetch the full Claude credential row by ID.
pub async fn get_row_by_id(pool: &SqlitePool, id: i64) -> Result<Option<CredentialRow>> {
    let row: Option<CredentialRow> =
        sqlx::query_as("SELECT * FROM credentials WHERE id = ? AND provider = 'claude'")
            .bind(id)
            .fetch_optional(pool)
            .await?;
    Ok(row)
}

/// Look up a credential id by `label`. Used by add paths to
/// pre-empt the table-wide `label TEXT NOT NULL UNIQUE` constraint with a
/// typed conflict instead of a generic SQL error.
pub async fn find_by_label(pool: &SqlitePool, label: &str) -> Result<Option<i64>> {
    let row: Option<(i64,)> = sqlx::query_as("SELECT id FROM credentials WHERE label = ?")
        .bind(label)
        .fetch_optional(pool)
        .await?;
    let row = row.map(|(id,)| id);
    Ok(row)
}

/// Get the access token for a Claude credential by ID.
pub async fn get_token_by_id(pool: &SqlitePool, id: i64) -> Result<Option<String>> {
    let token: Option<(String,)> =
        sqlx::query_as("SELECT access_token FROM credentials WHERE id = ? AND provider = 'claude'")
            .bind(id)
            .fetch_optional(pool)
            .await?;
    Ok(token.map(|t| t.0))
}

/// Insert a credential. Returns the row ID.
pub async fn insert(
    pool: &SqlitePool,
    label: &str,
    access_token: &str,
    expires_at: Option<i64>,
) -> Result<i64> {
    let id: i64 = sqlx::query_scalar(
        "INSERT INTO credentials (label, access_token, expires_at, updated_at)
        VALUES (?1, ?2, ?3, datetime('now'))
        RETURNING id",
    )
    .bind(label)
    .bind(access_token)
    .bind(expires_at)
    .fetch_one(pool)
    .await?;
    Ok(id)
}

/// Delete a Claude credential by ID. Returns true if a row was deleted.
/// Also nulls `credential_id` on any existing `cc_sessions` rows so there
/// are no orphaned FK references (SQLite `ALTER TABLE` can't add ON DELETE
/// SET NULL retroactively, so we enforce it in the delete path).
pub async fn delete(pool: &SqlitePool, id: i64) -> Result<bool> {
    let mut tx = pool.begin().await?;
    sqlx::query("UPDATE cc_sessions SET credential_id = NULL WHERE credential_id = ?")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    let result = sqlx::query("DELETE FROM credentials WHERE id = ? AND provider = 'claude'")
        .bind(id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(result.rows_affected() > 0)
}

/// Set rate-limit cooldown on a credential.
pub async fn set_rate_limit_cooldown(
    pool: &SqlitePool,
    id: i64,
    until_epoch_secs: i64,
) -> Result<bool> {
    let result = sqlx::query(
        "UPDATE credentials SET rate_limit_cooldown_until = ?1, updated_at = datetime('now')
         WHERE id = ?2",
    )
    .bind(until_epoch_secs)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Clear the rate-limit cooldown on a specific credential. Used when a
/// proactive probe returns `allowed` for a credential that was previously
/// rate-limited — the server recovered before the capped cooldown window
/// ended, so we let it be picked again immediately.
pub async fn clear_rate_limit_cooldown(pool: &SqlitePool, id: i64) -> Result<bool> {
    let result = sqlx::query(
        "UPDATE credentials SET rate_limit_cooldown_until = NULL, updated_at = datetime('now')
         WHERE id = ?1 AND rate_limit_cooldown_until IS NOT NULL",
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Persist a probe snapshot on a credential row.
///
/// Writes the nine usage columns atomically. Callers that see
/// `snapshot.unified_status == Rejected` must also call
/// [`set_rate_limit_cooldown`] (directly or via the existing
/// `credential_rate_limit::activate`) so `pick_for_worker` filtering keeps
/// one source of truth.
pub async fn set_usage_snapshot(
    pool: &SqlitePool,
    id: i64,
    snapshot: &UsageSnapshot,
) -> Result<bool> {
    let result = sqlx::query(
        "UPDATE credentials SET
            five_hour_utilization = ?1,
            five_hour_reset_at = ?2,
            five_hour_status = ?3,
            seven_day_utilization = ?4,
            seven_day_reset_at = ?5,
            seven_day_status = ?6,
            unified_status = ?7,
            representative_claim = ?8,
            last_probed_at = ?9,
            updated_at = datetime('now')
         WHERE id = ?10",
    )
    .bind(snapshot.five_hour.utilization)
    .bind(snapshot.five_hour.reset_at)
    .bind(snapshot.five_hour.status.as_str())
    .bind(snapshot.seven_day.utilization)
    .bind(snapshot.seven_day.reset_at)
    .bind(snapshot.seven_day.status.as_str())
    .bind(snapshot.unified_status.as_str())
    .bind(snapshot.representative_claim.as_deref())
    .bind(snapshot.probed_at)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Mark a credential as expired by setting `expires_at` to the current time
/// (Unix ms). Used after a probe returns 401; the user must re-login.
pub async fn mark_expired(pool: &SqlitePool, id: i64) -> Result<bool> {
    let now_ms = time::OffsetDateTime::now_utc().unix_timestamp() * 1000;
    let result = sqlx::query(
        "UPDATE credentials SET expires_at = ?1, updated_at = datetime('now')
         WHERE id = ?2",
    )
    .bind(now_ms)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

/// Sum `cost_usd` across completed `cc_sessions` rows for a credential that
/// created or finished after `since_unix_secs`. Feeds the between-probe
/// "cost since probe" indicator.
///
/// Returns `0.0` when no matching sessions exist, the credential was never
/// probed, or the query fails (a log line is emitted in the failure case).
pub async fn cost_since(pool: &SqlitePool, credential_id: i64, since_unix_secs: i64) -> f64 {
    let since_rfc3339 = time::OffsetDateTime::from_unix_timestamp(since_unix_secs)
        .map(|dt| {
            dt.format(&time::format_description::well_known::Rfc3339)
                .unwrap_or_default()
        })
        .unwrap_or_default();
    let query = sqlx::query_scalar::<_, Option<f64>>(
        "SELECT COALESCE(SUM(cost_usd), 0.0) FROM cc_sessions
         WHERE credential_id = ?1
           AND cost_usd IS NOT NULL
           AND created_at >= ?2",
    )
    .bind(credential_id)
    .bind(since_rfc3339);
    match query.fetch_optional(pool).await {
        Ok(sum) => sum.flatten().unwrap_or(0.0),
        Err(e) => {
            tracing::warn!(
                module = "credentials",
                credential_id,
                error = %e,
                "cost_since query failed; returning 0.0"
            );
            0.0
        }
    }
}

/// Pick the best credential: not expired, not rate-limited, fewest active
/// (running) sessions. Returns (id, access_token).
///
/// `caller_filter` narrows which running sessions count toward the
/// active-session tally. Pass `Some("worker")` when spawning a worker so
/// only other worker sessions influence the pick (workers dominate token
/// spend). Pass `None` to count all running sessions (default for
/// lightweight callers).
pub async fn pick_for_worker(
    pool: &SqlitePool,
    caller_filter: Option<&str>,
) -> Result<Option<(i64, String)>> {
    let now_ms = time::OffsetDateTime::now_utc().unix_timestamp() * 1000;
    let now_secs = now_ms / 1000;

    // Keep the provider filter for databases that still contain stale Codex
    // rows from the removed Credentials-page Codex account feature.
    let row: Option<(i64, String)> = sqlx::query_as(
        "SELECT c.id, c.access_token
         FROM credentials c
         LEFT JOIN (
             SELECT credential_id, COUNT(*) AS active
             FROM cc_sessions
             WHERE status = 'running' AND credential_id IS NOT NULL
               AND (?3 IS NULL OR caller = ?3)
             GROUP BY credential_id
         ) s ON s.credential_id = c.id
         WHERE c.provider = 'claude'
           AND (c.expires_at IS NULL OR c.expires_at > ?1)
           AND (c.rate_limit_cooldown_until IS NULL OR c.rate_limit_cooldown_until <= ?2)
         ORDER BY
            COALESCE(s.active, 0) ASC,
            COALESCE(c.five_hour_utilization, 0.0) ASC,
            c.id ASC
         LIMIT 1",
    )
    .bind(now_ms)
    .bind(now_secs)
    .bind(caller_filter)
    .fetch_optional(pool)
    .await?;
    Ok(row)
}

/// Seconds remaining until a specific credential leaves cooldown.
/// Returns 0 if the credential isn't cooling down (or doesn't exist).
/// Propagates DB errors — a transient failure coerced to 0 used to
/// look identical to "no cooldown" and led captain to reuse a
/// rate-limited key.
pub async fn cooldown_remaining_secs(pool: &SqlitePool, id: i64) -> anyhow::Result<i64> {
    let now_secs = time::OffsetDateTime::now_utc().unix_timestamp();
    let row: Option<(Option<i64>,)> =
        sqlx::query_as("SELECT rate_limit_cooldown_until FROM credentials WHERE id = ?")
            .bind(id)
            .fetch_optional(pool)
            .await
            .map_err(|e| anyhow::anyhow!("cooldown query failed for credential {id}: {e}"))?;
    Ok(match row {
        Some((Some(until),)) if until > now_secs => until - now_secs,
        _ => 0,
    })
}

/// Seconds until the earliest credential leaves cooldown. Returns 0 when no
/// credentials are cooling down. Propagates DB errors for the same reason
/// as `cooldown_remaining_secs`.
pub async fn earliest_cooldown_remaining_secs(pool: &SqlitePool) -> anyhow::Result<i64> {
    let now_secs = time::OffsetDateTime::now_utc().unix_timestamp();
    let row: Option<(Option<i64>,)> = sqlx::query_as(
        "SELECT MIN(rate_limit_cooldown_until) FROM credentials
         WHERE rate_limit_cooldown_until IS NOT NULL AND rate_limit_cooldown_until > ?",
    )
    .bind(now_secs)
    .fetch_optional(pool)
    .await
    .map_err(|e| anyhow::anyhow!("earliest-cooldown query failed: {e}"))?;
    Ok(match row {
        Some((Some(until),)) => until - now_secs,
        _ => 0,
    })
}

/// Clear all active credential cooldowns. Used by the manual resume API so the
/// next tick can pick a credential up immediately.
pub async fn clear_all_cooldowns(pool: &SqlitePool) -> Result<u64> {
    let result = sqlx::query(
        "UPDATE credentials SET rate_limit_cooldown_until = NULL, updated_at = datetime('now')
         WHERE rate_limit_cooldown_until IS NOT NULL",
    )
    .execute(pool)
    .await?;
    Ok(result.rows_affected())
}

/// Count running sessions grouped by credential_id (for diagnostics).
pub async fn active_counts(pool: &SqlitePool) -> Result<HashMap<i64, u32>> {
    let rows: Vec<(i64, i64)> = sqlx::query_as(
        "SELECT credential_id, COUNT(*) FROM cc_sessions
         WHERE status = 'running' AND credential_id IS NOT NULL
         GROUP BY credential_id",
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(id, count)| (id, count as u32))
        .collect())
}
