//! Subscription cache and persisted daily attempt guard. Contains no auth material.
use anyhow::Result;
use api_types::ClaudeSubscriptionInfo;
use sqlx::SqlitePool;

use super::claude_subscription_probe::SubscriptionSnapshot;

#[derive(sqlx::FromRow)]
pub(crate) struct SubscriptionRow {
    pub credential_id: i64,
    pub account_uuid: String,
    pub plan_name: Option<String>,
    pub billing_interval: Option<String>,
    pub subscription_status: Option<String>,
    pub renews_at: Option<i64>,
    pub ends_at: Option<i64>,
    pub ends_before: Option<String>,
    pub next_charge_date: Option<String>,
    pub checked_at: Option<i64>,
    pub attempted_at: i64,
    pub error: Option<String>,
    pub keychain_access_required: bool,
}

impl SubscriptionRow {
    pub(crate) fn into_info(self) -> ClaudeSubscriptionInfo {
        ClaudeSubscriptionInfo {
            plan_name: self.plan_name,
            billing_interval: self.billing_interval,
            status: self.subscription_status,
            renews_at: self.renews_at,
            ends_at: self.ends_at,
            ends_before: self.ends_before,
            next_charge_date: self.next_charge_date,
            checked_at: self.checked_at,
            attempted_at: Some(self.attempted_at),
            error: self.error,
            keychain_access_required: self.keychain_access_required,
        }
    }
}

pub(crate) async fn list(pool: &SqlitePool) -> Result<Vec<SubscriptionRow>> {
    Ok(sqlx::query_as("SELECT * FROM claude_subscriptions")
        .fetch_all(pool)
        .await?)
}

pub(crate) async fn get(pool: &SqlitePool, id: i64) -> Result<Option<SubscriptionRow>> {
    Ok(
        sqlx::query_as("SELECT * FROM claude_subscriptions WHERE credential_id=?")
            .bind(id)
            .fetch_optional(pool)
            .await?,
    )
}

/// Claim before network I/O, including failures and daemon restarts. Account
/// changes clear the previous account's cached billing data atomically.
pub(crate) async fn claim(
    pool: &SqlitePool,
    id: i64,
    account: &str,
    now: i64,
    force: bool,
) -> Result<bool> {
    let result = sqlx::query(
        "INSERT INTO claude_subscriptions (credential_id, account_uuid, attempted_at, error)
         VALUES (?1, ?2, ?3, 'Subscription refresh did not complete. Refresh the plan to retry.')
         ON CONFLICT(credential_id) DO UPDATE SET
           plan_name=CASE WHEN account_uuid=?2 THEN plan_name ELSE NULL END,
           billing_interval=CASE WHEN account_uuid=?2 THEN billing_interval ELSE NULL END,
           subscription_status=CASE WHEN account_uuid=?2 THEN subscription_status ELSE NULL END,
           renews_at=CASE WHEN account_uuid=?2 THEN renews_at ELSE NULL END,
           ends_at=CASE WHEN account_uuid=?2 THEN ends_at ELSE NULL END,
           ends_before=CASE WHEN account_uuid=?2 THEN ends_before ELSE NULL END,
           next_charge_date=CASE WHEN account_uuid=?2 THEN next_charge_date ELSE NULL END,
           checked_at=CASE WHEN account_uuid=?2 THEN checked_at ELSE NULL END,
           account_uuid=?2, attempted_at=?3, error=excluded.error, keychain_access_required=0
         WHERE ?4 OR account_uuid<>?2 OR attempted_at<=?3-86400000",
    )
    .bind(id)
    .bind(account)
    .bind(now)
    .bind(force)
    .execute(pool)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub(crate) async fn succeed(
    pool: &SqlitePool,
    id: i64,
    attempted: i64,
    snapshot: &SubscriptionSnapshot,
    now: i64,
) -> Result<()> {
    sqlx::query(
        "UPDATE claude_subscriptions SET plan_name=?1, billing_interval=?2,
         subscription_status=?3, renews_at=?4, ends_at=?5, checked_at=?6, error=NULL, keychain_access_required=0,
         ends_before=?10, next_charge_date=?11
         WHERE credential_id=?7 AND account_uuid=?8 AND attempted_at=?9",
    ).bind(&snapshot.plan_name).bind(&snapshot.billing_interval).bind(&snapshot.status)
        .bind(snapshot.renews_at).bind(snapshot.ends_at).bind(now).bind(id)
        .bind(&snapshot.account_uuid).bind(attempted)
        .bind(&snapshot.ends_before).bind(&snapshot.next_charge_date).execute(pool).await?;
    Ok(())
}

pub(crate) async fn fail(
    pool: &SqlitePool,
    id: i64,
    attempted: i64,
    error: &str,
    keychain_access_required: bool,
) -> Result<()> {
    sqlx::query("UPDATE claude_subscriptions SET error=?1, keychain_access_required=?4 WHERE credential_id=?2 AND attempted_at=?3")
        .bind(error).bind(id).bind(attempted).bind(keychain_access_required).execute(pool).await?;
    Ok(())
}
