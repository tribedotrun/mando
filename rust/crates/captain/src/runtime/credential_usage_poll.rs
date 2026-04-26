//! Proactive credential usage poller.
//!
//! Runs as an independent tokio task alongside the captain tick loop. Every
//! tick it:
//!
//! 1. Lists every stored credential.
//! 2. For each credential that is not expired and not within the throttle
//!    window, pings `/v1/messages` and reads the
//!    `anthropic-ratelimit-unified-*` headers to capture live utilization.
//! 3. Persists the snapshot to the `credentials` row (columns added in
//!    migration 026).
//! 4. Unifies with the reactive rate-limit path: when the snapshot's
//!    `unified_status == Rejected`, calls
//!    [`credential_rate_limit::activate`] so `pick_for_worker` filtering
//!    keeps one source of truth.
//! 5. Emits `BusEvent::Credentials` so the Electron UI refetches live.
//!
//! Cadence is a flat 10 minutes for all credentials regardless of
//! provider or utilization (PR #1006 simplification). A per-credential
//! throttle prevents redundant back-to-back probes when the manual
//! refresh endpoint fires at the same time as the scheduled tick.

use std::sync::Arc;
use std::time::Duration;

use sqlx::SqlitePool;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use global_bus::EventBus;
use settings::credentials::{self, CredentialRow};
use settings::usage_probe::{ProbeError, RateLimitStatus, UsageSnapshot};

use super::credential_rate_limit;

/// Sleep between poll ticks. Flat for all providers and all utilizations.
const TICK_INTERVAL: Duration = Duration::from_secs(600);
/// Do not re-probe a credential whose `last_probed_at` is within this window.
/// Protects against manual-refresh + scheduled-tick collisions.
const PER_CREDENTIAL_THROTTLE_SECS: i64 = 60;
/// Initial delay before the first probe (let the daemon finish booting).
const STARTUP_DELAY: Duration = Duration::from_secs(15);

/// Run the poller until `cancel` fires.
///
/// Spawned from `mando-gateway::background_tasks::spawn_credential_usage_poll`.
#[tracing::instrument(skip_all)]
pub async fn run(pool: SqlitePool, bus: Arc<EventBus>, cancel: CancellationToken) {
    info!(
        module = "captain",
        "credential usage poll started (interval={}s)",
        TICK_INTERVAL.as_secs()
    );
    tokio::select! {
        _ = tokio::time::sleep(STARTUP_DELAY) => {}
        _ = cancel.cancelled() => {
            info!(module = "captain", "credential usage poll cancelled during warm-up");
            return;
        }
    }

    loop {
        if cancel.is_cancelled() {
            break;
        }

        if let Err(e) = tick_once(&pool, &bus).await {
            warn!(
                module = "captain",
                error = %e,
                "credential usage poll tick failed"
            );
        }

        tokio::select! {
            _ = tokio::time::sleep(TICK_INTERVAL) => {}
            _ = cancel.cancelled() => break,
        }
    }

    info!(module = "captain", "credential usage poll stopped");
}

/// Probe every eligible credential once. Persistence and bus emission
/// happen inline; the caller does not need a return value.
async fn tick_once(pool: &SqlitePool, bus: &EventBus) -> anyhow::Result<()> {
    let rows = credentials::list_all(pool).await?;
    if rows.is_empty() {
        return Ok(());
    }
    let now_secs = time::OffsetDateTime::now_utc().unix_timestamp();
    let mut dirty = false;

    for row in rows {
        if !should_probe(&row, now_secs) {
            continue;
        }

        match probe_and_persist(pool, &row).await {
            Ok(snapshot) => {
                dirty = true;
                info!(
                    module = "captain",
                    credential_id = row.id,
                    label = %row.label,
                    five_hour_pct = snapshot.five_hour.utilization * 100.0,
                    seven_day_pct = snapshot.seven_day.utilization * 100.0,
                    unified_status = snapshot.unified_status.as_str(),
                    "credential usage probed"
                );
            }
            Err(ProbeError::Unauthorized) => {
                warn!(
                    module = "captain",
                    credential_id = row.id,
                    label = %row.label,
                    "credential probe returned 401; marking expired"
                );
                if let Err(e) = credentials::mark_expired(pool, row.id).await {
                    warn!(module = "captain", credential_id = row.id, error = %e,
                          "failed to mark credential expired");
                } else {
                    dirty = true;
                }
            }
            // Persist errors mean the snapshot arrived but didn't stick —
            // worth a warning because the poll throttle and pre-spawn
            // staleness check both rely on last_probed_at advancing.
            Err(ProbeError::Persist(e)) => {
                warn!(
                    module = "captain",
                    credential_id = row.id,
                    error = %e,
                    "credential snapshot persist failed; throttle state will not advance"
                );
            }
            // Parse errors mean the upstream API response shape drifted —
            // not a transient blip. Surface at warn so it's visible.
            Err(ProbeError::Parse(e)) => {
                warn!(
                    module = "captain",
                    credential_id = row.id,
                    error = %e,
                    "credential probe response missing expected headers"
                );
            }
            Err(e) => {
                // Bumped from debug to warn so a sustained Codex/Claude
                // probe outage is visible at the default log level.
                // Transient blips will produce noise, but operator
                // visibility into upstream rate-limit/token outages wins.
                warn!(
                    module = "captain",
                    credential_id = row.id,
                    error = %e,
                    "credential probe transient failure"
                );
            }
        }
    }

    if dirty {
        bus.send(global_bus::BusPayload::Credentials(None));
    }
    Ok(())
}

/// Returns whether we should probe this credential right now.
///
/// We intentionally probe credentials that are in cooldown: `seven_day`
/// rejections cap their cooldown at 1h (see `credential_rate_limit.rs`),
/// but the server can recover much sooner as old usage ages out of the
/// sliding window. Re-probing the cooldown'd credential is the only way
/// to detect that early recovery. An expired credential still skips.
pub(crate) fn should_probe(row: &CredentialRow, now_secs: i64) -> bool {
    !is_expired(row, now_secs) && !recently_probed(row, now_secs)
}

fn is_expired(row: &CredentialRow, now_secs: i64) -> bool {
    let now_ms = now_secs.saturating_mul(1000);
    row.expires_at.is_some_and(|ea| ea <= now_ms)
}

fn recently_probed(row: &CredentialRow, now_secs: i64) -> bool {
    row.last_probed_at
        .is_some_and(|last| now_secs - last < PER_CREDENTIAL_THROTTLE_SECS)
}

/// Probe a single credential and persist the result.
///
/// Shared between the scheduled poller and the manual-refresh HTTP route.
/// On `Rejected`, unifies with the existing reactive path by calling
/// [`credential_rate_limit::activate`]. On `Allowed` for a credential that
/// was previously cooling down, clears the cooldown so the next
/// `pick_for_worker` picks it up immediately — the key reason the poller
/// bothers to probe cooldown'd credentials.
///
/// Persist failures surface as [`ProbeError::Persist`] rather than being
/// swallowed, because callers rely on `last_probed_at` advancing to make
/// correct throttle and staleness decisions.
#[tracing::instrument(skip_all)]
pub async fn probe_and_persist(
    pool: &SqlitePool,
    row: &CredentialRow,
) -> Result<UsageSnapshot, ProbeError> {
    // Boxed to keep the outer future shape simple; provider_probe holds
    // h2/hyper internals whose Send-bound trait recursion blows the
    // default limit when inlined into the tracing::instrument span.
    let snapshot = Box::pin(settings::provider_probe::probe(pool, row)).await?;
    credentials::set_usage_snapshot(pool, row.id, &snapshot)
        .await
        .map_err(|e| {
            warn!(
                module = "captain",
                credential_id = row.id,
                error = %e,
                "failed to persist credential usage snapshot"
            );
            ProbeError::Persist(e.to_string())
        })?;
    match snapshot.unified_status {
        RateLimitStatus::Rejected => {
            // Pick the reset belonging to whichever window is actually
            // binding. Falling back to the larger of the two guards
            // against a claim we don't recognize: for a `seven_day`
            // rejection the 5h reset may already be in the past, which
            // `compute_cooldown_until` would then drop to a 10-minute
            // default — far too short for a weekly cap.
            let reset_at = match snapshot.representative_claim.as_deref() {
                Some("five_hour") => snapshot.five_hour.reset_at,
                Some(s) if s.starts_with("seven_day") => snapshot.seven_day.reset_at,
                _ => snapshot.five_hour.reset_at.max(snapshot.seven_day.reset_at),
            };
            let reset_at = reset_at.max(0) as u64;
            let claim = snapshot.representative_claim.as_deref();
            credential_rate_limit::activate(pool, row.id, Some(reset_at), claim).await;
        }
        RateLimitStatus::Allowed
        | RateLimitStatus::AllowedWarning
        | RateLimitStatus::Unknown(_) => {
            if row.rate_limit_cooldown_until.is_some() {
                match credentials::clear_rate_limit_cooldown(pool, row.id).await {
                    Ok(true) => info!(
                        module = "captain",
                        credential_id = row.id,
                        "proactive probe cleared stale rate-limit cooldown"
                    ),
                    Ok(false) => {}
                    Err(e) => warn!(
                        module = "captain",
                        credential_id = row.id,
                        error = %e,
                        "failed to clear credential rate-limit cooldown"
                    ),
                }
            }
        }
    }
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row_with(
        expires_at: Option<i64>,
        cooldown: Option<i64>,
        last_probed: Option<i64>,
    ) -> CredentialRow {
        CredentialRow {
            id: 1,
            label: "t".into(),
            access_token: "x".into(),
            expires_at,
            rate_limit_cooldown_until: cooldown,
            created_at: String::new(),
            updated_at: String::new(),
            five_hour_utilization: None,
            five_hour_reset_at: None,
            five_hour_status: None,
            seven_day_utilization: None,
            seven_day_reset_at: None,
            seven_day_status: None,
            unified_status: None,
            representative_claim: None,
            last_probed_at: last_probed,
            provider: "claude".into(),
            refresh_token: None,
            id_token: None,
            account_id: None,
            plan_type: None,
            credits_balance: None,
            credits_unlimited: 0,
        }
    }

    #[test]
    fn should_probe_fresh_credential() {
        let row = row_with(None, None, None);
        assert!(should_probe(&row, 1_000_000));
    }

    #[test]
    fn should_skip_expired() {
        let now_secs = 1_000_000;
        let expired_ms = (now_secs - 10) * 1000;
        let row = row_with(Some(expired_ms), None, None);
        assert!(!should_probe(&row, now_secs));
    }

    #[test]
    fn should_probe_cooldown_for_early_recovery() {
        let now_secs = 1_000_000;
        let row = row_with(None, Some(now_secs + 600), None);
        assert!(should_probe(&row, now_secs));
    }

    #[test]
    fn should_skip_recently_probed() {
        let now_secs = 1_000_000;
        let row = row_with(None, None, Some(now_secs - 10));
        assert!(!should_probe(&row, now_secs));
    }

    #[test]
    fn should_probe_after_throttle_window() {
        let now_secs = 1_000_000;
        let row = row_with(
            None,
            None,
            Some(now_secs - PER_CREDENTIAL_THROTTLE_SECS - 1),
        );
        assert!(should_probe(&row, now_secs));
    }
}
