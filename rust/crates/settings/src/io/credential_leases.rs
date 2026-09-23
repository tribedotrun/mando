//! Durable, expiring external process leases. No captain ownership.
use anyhow::Result;
use api_types::{CredentialLeaseCount, CredentialLeaseHeartbeatRequest};
use sqlx::{SqliteConnection, SqlitePool};

pub(crate) const LEASE_SECONDS: i64 = 90;

/// Count each external launch and managed sessions without matching live launches, plus reservations.
/// The caller's own launch is excluded while scoring its replacement.
pub(crate) async fn counts(
    connection: &mut SqliteConnection,
    now: i64,
    exclude_launch: &str,
) -> Result<Vec<CredentialLeaseCount>> {
    let rows: Vec<(i64, i64, i64)> = sqlx::query_as(
        "WITH live AS (
            SELECT credential_id, 'launch:' || launch_id AS identity
            FROM claude_launch_leases
            WHERE expires_at > ?1 AND credential_id IS NOT NULL AND launch_id != ?2
            UNION
            SELECT credential_id, session_id AS identity FROM cc_sessions s
            WHERE status = 'running' AND credential_id IS NOT NULL
              AND NOT EXISTS (SELECT 1 FROM claude_launch_leases l
                  WHERE l.session_id = s.session_id AND l.expires_at > ?1)
        ), pending AS (
            SELECT pending_credential_id AS credential_id, COUNT(*) AS n
            FROM claude_launch_leases
            WHERE pending_expires_at > ?1 AND launch_id != ?2
            GROUP BY pending_credential_id
        )
        SELECT c.id, (SELECT COUNT(*) FROM live WHERE live.credential_id = c.id),
               COALESCE(p.n, 0)
        FROM credentials c LEFT JOIN pending p ON p.credential_id = c.id
        WHERE c.provider = 'claude' ORDER BY c.id",
    )
    .bind(now)
    .bind(exclude_launch)
    .fetch_all(connection)
    .await?;
    Ok(rows
        .into_iter()
        .map(
            |(credential_id, live_sessions, pending_launches)| CredentialLeaseCount {
                credential_id,
                live_sessions,
                pending_launches,
            },
        )
        .collect())
}

pub(crate) async fn reserve(
    connection: &mut SqliteConnection,
    launch: &str,
    id: i64,
    now: i64,
) -> Result<()> {
    sqlx::query("INSERT INTO claude_launch_leases (launch_id, expires_at, pending_credential_id, pending_expires_at)
        VALUES (?1, 0, ?2, ?3) ON CONFLICT(launch_id) DO UPDATE SET
        pending_credential_id = excluded.pending_credential_id, pending_expires_at = excluded.pending_expires_at")
        .bind(launch).bind(id).bind(now + LEASE_SECONDS).execute(&mut *connection).await?;
    sqlx::query("UPDATE credentials SET last_picked_at = ?1 WHERE id = ?2")
        .bind(now)
        .bind(id)
        .execute(connection)
        .await?;
    Ok(())
}

pub(crate) async fn heartbeat(
    pool: &SqlitePool,
    request: &CredentialLeaseHeartbeatRequest,
) -> Result<i64> {
    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    let expiry = now + LEASE_SECONDS;
    let mut tx = pool.begin_with("BEGIN IMMEDIATE").await?;
    let valid: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM credentials WHERE id = ? AND provider = 'claude')",
    )
    .bind(request.credential_id)
    .fetch_one(&mut *tx)
    .await?;
    anyhow::ensure!(valid, "Claude credential does not exist");
    sqlx::query("INSERT INTO claude_launch_leases
        (launch_id, credential_id, session_id, pid, cwd, model, expires_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        ON CONFLICT(launch_id) DO UPDATE SET credential_id=excluded.credential_id,
        session_id=excluded.session_id, pid=excluded.pid, cwd=excluded.cwd,
        model=excluded.model, expires_at=excluded.expires_at,
        pending_credential_id=CASE WHEN ?8 AND pending_credential_id=?2 THEN NULL ELSE pending_credential_id END,
        pending_expires_at=CASE WHEN ?8 AND pending_credential_id=?2 THEN NULL ELSE CASE WHEN pending_credential_id IS NOT NULL THEN ?7 ELSE pending_expires_at END END")
        .bind(&request.launch_id).bind(request.credential_id).bind(&request.session_id)
        .bind(i64::from(request.pid)).bind(&request.cwd).bind(&request.model).bind(expiry)
        .bind(request.confirm_reservation).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(expiry)
}

pub(crate) async fn release(pool: &SqlitePool, launch: &str) -> Result<()> {
    sqlx::query("DELETE FROM claude_launch_leases WHERE launch_id = ?")
        .bind(launch)
        .execute(pool)
        .await?;
    Ok(())
}
