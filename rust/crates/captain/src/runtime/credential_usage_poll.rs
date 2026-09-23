//! Proactive credential usage poller.
//!
//! Runs as an independent tokio task alongside the captain tick loop. Every
//! tick it:
//!
//! 1. Lists every stored credential.
//! 2. For each credential that is not expired and not within the throttle
//!    window, captures subscription usage through its provider.
//! 3. Persists the snapshot to the `credentials` row (columns added in
//!    migration 026).
//! 4. Unifies with the reactive rate-limit path: when the snapshot's
//!    `unified_status == Rejected`, calls
//!    [`credential_rate_limit::activate`] so `pick_for_worker` filtering
//!    keeps one source of truth.
//! 5. Emits `BusPayload::Credentials` so the Electron UI refetches live.
//! 6. For an idle Codex credential (zero usage, no reset clock running),
//!    fires a usage warm-up through [`credential_codex_warmup`] so the
//!    rolling windows start counting before real work lands on it.
//!
//! Claude usage refreshes every three hours; Codex every ten minutes.
//! Expired cooldowns trigger a fresh probe before a credential is reused.
//! The manual refresh endpoint remains available between scheduled probes.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use sqlx::SqlitePool;
use tokio_util::sync::CancellationToken;
use tracing::{info, warn};

use global_bus::EventBus;
use settings::credentials::{self, CredentialRow};
use settings::usage_probe::{ProbeError, RateLimitStatus, UsageSnapshot};
use settings::SettingsRuntime;

use super::credential_codex_warmup::{self, WarmupAttempts};
use super::credential_rate_limit;

/// Check scheduling deadlines every ten minutes.
const TICK_INTERVAL: Duration = Duration::from_secs(600);
const CLAUDE_REFRESH_SECS: i64 = 3 * 60 * 60;
const CODEX_REFRESH_SECS: i64 = 10 * 60;
/// Do not re-probe a credential whose `last_probed_at` is within this window.
/// Protects against manual-refresh + scheduled-tick collisions.
const PER_CREDENTIAL_THROTTLE_SECS: i64 = 60;
/// Initial delay before the first probe (let the daemon finish booting).
const STARTUP_DELAY: Duration = Duration::from_secs(15);

/// Run the poller until `cancel` fires.
///
/// Spawned from `mando-gateway::background_tasks::spawn_credential_usage_poll`.
#[tracing::instrument(skip_all)]
pub async fn run(
    pool: SqlitePool,
    bus: Arc<EventBus>,
    settings: Arc<SettingsRuntime>,
    cancel: CancellationToken,
) {
    let mut last_attempts = HashMap::new();
    let mut warmup_attempts = WarmupAttempts::default();
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

        let tick_result = tokio::select! {
            result = tick_once(&pool, &bus, &settings, &mut last_attempts, &mut warmup_attempts) => result,
            _ = cancel.cancelled() => break,
        };
        if let Err(e) = tick_result {
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
async fn tick_once(
    pool: &SqlitePool,
    bus: &EventBus,
    settings: &SettingsRuntime,
    last_attempts: &mut HashMap<i64, i64>,
    warmup_attempts: &mut WarmupAttempts,
) -> anyhow::Result<()> {
    let rows = credentials::list_all(pool).await?;
    last_attempts.retain(|id, _| rows.iter().any(|row| row.id == *id));
    warmup_attempts.retain_ids(|id| rows.iter().any(|row| row.id == id));
    if rows.is_empty() {
        return Ok(());
    }
    let now_secs = time::OffsetDateTime::now_utc().unix_timestamp();
    let mut dirty = false;

    for row in rows {
        if !should_probe(&row, now_secs) {
            continue;
        }
        if !cooldown_expired(&row, now_secs)
            && last_attempts
                .get(&row.id)
                .is_some_and(|last| now_secs - last < refresh_interval(&row))
        {
            continue;
        }
        // Failed or incomplete sessions also consume quota. Keep their
        // scheduled retry at the same cadence without claiming a new snapshot.
        last_attempts.insert(row.id, now_secs);

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
                credential_codex_warmup::maybe_warm(
                    settings,
                    &row,
                    &snapshot,
                    warmup_attempts,
                    now_secs,
                )
                .await;
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
                    // `should_probe` excludes expired rows going forward, so
                    // this branch only runs once per credential — the
                    // transition into expired/auth-dead, not every tick.
                    if row.provider == "codex" {
                        notify_codex_credential_dead(bus, row.id, &row.label);
                    }
                }
            }
            Err(ProbeError::RateLimited { .. }) => {
                // probe_and_persist applied the rejection's cooldown even
                // when the CLI could not return utilization windows.
                dirty = true;
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
                    "credential probe response missing expected usage windows"
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

/// Emit a user-visible notification on the transition into
/// expired/auth-dead for a Codex pool credential. Sent directly on the bus
/// (bypassing `captain::runtime::notify::Notifier`, which requires a full
/// `CaptainRuntime` instance this standalone poller — and the pick-time
/// caller in `transport-http` — do not have) — the same low-level pattern
/// the auto-tick "degraded" alert uses in `daemon_background.rs`.
///
/// Exported (Fix 5) so the `POST /api/credentials/codex/pick` route can
/// reuse the exact same message format instead of duplicating the string
/// when the pick walk itself marks a credential expired.
/// `task_key` stays `codex-credential-expired:{id}` regardless of caller so
/// poll- and pick-triggered duplicates coalesce in the notification store.
pub fn notify_codex_credential_dead(bus: &EventBus, credential_id: i64, label: &str) {
    let payload = api_types::NotificationPayload {
        message: format!(
            "Codex credential '{label}' login session revoked or expired; re-add it from a \
             fresh throwaway-home login."
        ),
        level: api_types::NotifyLevel::High,
        kind: api_types::NotificationKind::Generic,
        task_key: Some(format!("codex-credential-expired:{credential_id}")),
        reply_markup: None,
    };
    bus.send(global_bus::BusPayload::Notification(payload));
}

/// Returns whether we should probe this credential right now.
///
/// Active cooldowns follow the normal refresh cadence to detect early
/// recovery. Expired cooldowns are due even when the normal refresh is not.
pub(crate) fn should_probe(row: &CredentialRow, now_secs: i64) -> bool {
    if is_expired(row, now_secs) || row.disabled_at.is_some() || recently_probed(row, now_secs) {
        return false;
    }
    if cooldown_expired(row, now_secs) {
        return true;
    }
    row.last_probed_at
        .is_none_or(|last| now_secs - last >= refresh_interval(row))
}

fn refresh_interval(row: &CredentialRow) -> i64 {
    if row.provider == "codex" {
        CODEX_REFRESH_SECS
    } else {
        CLAUDE_REFRESH_SECS
    }
}

fn cooldown_expired(row: &CredentialRow, now_secs: i64) -> bool {
    row.rate_limit_cooldown_until
        .is_some_and(|until| until > 0 && until <= now_secs)
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
    let snapshot = match settings::provider_probe::probe(pool, row).await {
        Err(error @ ProbeError::RateLimited { .. }) => {
            if let ProbeError::RateLimited {
                resets_at,
                ref claim,
            } = error
            {
                credential_rate_limit::activate(pool, row.id, resets_at, claim.as_deref()).await;
            }
            return Err(error);
        }
        result => result?,
    };
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
