//! Credential-aware CC invocation with rate-limit failover.
//!
//! This module is the authoritative entry point for any CC call that should
//! survive a per-credential 429. When the active credential returns
//! `api_error_status=429` ("You've hit your limit…"), the wrapper:
//!
//! 1. Cools down the failing credential so `pick_for_worker` excludes it
//!    from the next pick.
//! 2. Re-picks a healthy credential from the pool.
//! 3. Resumes the **same session transcript** under the new OAuth token via
//!    `--resume <sid>` so the expensive tool calls already paid for on the
//!    first attempt are not re-executed.
//! 4. If no healthy credential remains, surfaces
//!    `CcError::AllCredentialsExhausted { earliest_reset }` so the caller
//!    can park the task until the clock passes `earliest_reset`.
//!
//! Transient upstream errors (502/503/504/529) also retry inside this
//! wrapper with a fresh CC session per attempt and the same credential.
//!
//! Cross-credential resume is safe: CC's on-disk transcript
//! (`~/.claude/projects/<slug>/<sid>.jsonl`) has no auth-bound fields, and
//! the OAuth token is consumed at API request time (not session load
//! time). Resuming under a second token bills against that account.
//!
//! `run_with_credential_failover` is the single retry shape in the repo —
//! there is no other retry loop. Ambient-auth callers (no credential rows
//! configured) get no failover and fall through to a single attempt.
//!
//! See `captain::runtime::credential_rate_limit` for worker-stream
//! cooldown activation; that path shares the `compute_cooldown_until`
//! helper exported here.

use anyhow::Result;
use global_claude::{CcConfig, CcError, CcOneShot, CcResult, ErrorClass};
use sqlx::SqlitePool;
use tracing::{info, warn};

use super::credentials;

/// Cap cooldowns for long-window rate limits (seven_day, etc.) where the
/// server-reported `resets_at` is a window boundary days away — the API
/// usually recovers much sooner as old usage ages out of the sliding
/// window. Next probe tick re-opens the credential if so.
const LONG_WINDOW_MAX_COOLDOWN_SECS: u64 = 3600;

/// Fresh-session context the caller receives per attempt. The failover
/// wrapper calls the caller-provided builder with this so the caller can
/// stamp the right `--resume` id and credential for the current attempt.
pub struct FailoverContext {
    /// `(id, oauth_token)` of the credential to use, or `None` if no
    /// credentials are configured (ambient auth). On failover attempts this
    /// is always the newly-picked credential, never the prior one.
    pub credential: Option<(i64, String)>,
    /// When `Some`, the wrapper wants the caller to build a `--resume`
    /// config pointing at this session id. Set on failover attempts after
    /// the first. `None` on the first attempt.
    pub resume_session_id: Option<String>,
}

/// Maximum number of credential-failover hops before giving up and
/// returning `AllCredentialsExhausted`. Three is enough to walk through a
/// normal multi-credential pool without retrying the same credential.
const MAX_FAILOVERS: u32 = 3;

/// Maximum transient retries (502/503/504/529) on a single attempt. Short
/// because real transient outages from Anthropic clear in seconds; longer
/// delays just compound user-visible latency.
const MAX_TRANSIENT_RETRIES: u32 = 2;

/// Default pause window (seconds) when the credentials DB is
/// unreachable but we already know every credential is cooling down.
/// Ten minutes — long enough to avoid an immediate re-dispatch loop,
/// short enough that a transient DB blip does not strand the task.
const FALLBACK_PARK_SECS: i64 = 600;

/// Compute cooldown deadline (unix seconds) from rate-limit parameters.
///
/// `resets_at` is in unix seconds, `rate_limit_type` is the CC
/// `rateLimitType` string (e.g. `"five_hour"`, `"seven_day"`).
/// For `five_hour` limits `resets_at` is a tight upper bound and is used
/// directly. For everything else we cap at
/// [`LONG_WINDOW_MAX_COOLDOWN_SECS`] so a seven-day window boundary does
/// not park credentials for multiple days.
pub fn compute_cooldown_until(
    now: u64,
    resets_at: Option<u64>,
    rate_limit_type: Option<&str>,
) -> u64 {
    match resets_at {
        Some(ts) if ts > now => {
            let is_long_window = !matches!(rate_limit_type, Some("five_hour"));
            if is_long_window {
                now + (ts - now + 30).min(LONG_WINDOW_MAX_COOLDOWN_SECS)
            } else {
                ts + 30
            }
        }
        _ => now + 600,
    }
}

/// Mark a credential as rate-limited. Thin wrapper over
/// [`credentials::set_rate_limit_cooldown`] that applies
/// [`compute_cooldown_until`] and logs structured fields for obs.
#[tracing::instrument(skip_all)]
pub async fn rate_limit_activate(
    pool: &SqlitePool,
    credential_id: i64,
    resets_at: Option<u64>,
    rate_limit_type: Option<&str>,
) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let until = compute_cooldown_until(now, resets_at, rate_limit_type);

    match credentials::set_rate_limit_cooldown(pool, credential_id, until as i64).await {
        Ok(_) => warn!(
            module = "cc-failover",
            credential_id,
            until_epoch = until,
            cooldown_secs = until - now,
            rate_limit_type = rate_limit_type.unwrap_or("unknown"),
            "credential rate-limited"
        ),
        Err(e) => tracing::error!(
            module = "cc-failover",
            credential_id,
            error = %e,
            "failed to set credential rate limit cooldown"
        ),
    }
}

/// Clear a credential's rate-limit cooldown. Thin wrapper over
/// [`credentials::set_rate_limit_cooldown`] with `0` so the pick-filter
/// lets it back in immediately.
#[tracing::instrument(skip_all)]
pub async fn rate_limit_clear(pool: &SqlitePool, credential_id: i64) {
    match credentials::set_rate_limit_cooldown(pool, credential_id, 0).await {
        Ok(_) => info!(
            module = "cc-failover",
            credential_id, "credential rate limit cleared"
        ),
        Err(e) => tracing::error!(
            module = "cc-failover",
            credential_id,
            error = %e,
            "failed to clear credential rate limit cooldown"
        ),
    }
}

/// Run a CC oneshot with credential-aware failover and transient retries.
///
/// `build_config` is called once per attempt with the `FailoverContext` to
/// produce the final `CcConfig`; the caller is responsible for calling
/// `with_credential` on the builder and setting `session_id` /
/// `resume_session_id` from the context. All other builder state
/// (prompt, model, tools, cwd, timeout) is the caller's concern.
///
/// Returns the successful `CcResult` on any attempt, or:
/// - `CcError::AllCredentialsExhausted { earliest_reset }` if every
///   credential in the pool is in cooldown at pick time.
/// - The underlying `CcError` for non-rate-limit failures (client errors,
///   spawn failures, timeouts, stream closed, etc.) — these never retry.
#[tracing::instrument(skip_all, fields(caller = %caller))]
pub async fn run_with_credential_failover<F>(
    pool: &SqlitePool,
    caller: &str,
    prompt: &str,
    build_config: F,
) -> Result<CcResult<serde_json::Value>, CcError>
where
    F: Fn(FailoverContext) -> CcConfig,
{
    // Coarsen caller into the load-balancing bucket `pick_for_worker`
    // expects on its `caller_filter`. The DB `cc_sessions.caller` values
    // are specific (e.g. "planning-cc-r1", "scout-research") but the
    // picker wants coarse groups so concurrent sibling callers count
    // against the same bucket — otherwise two workers on the same
    // credential never see each other's load and pile onto the same
    // account until one 429s.
    let caller_bucket = caller_to_bucket(caller);
    // Pick the first credential. `None` here means one of two things:
    //   (a) no credential rows are configured -> ambient auth, no
    //       failover possible.
    //   (b) rows exist but every one is in cooldown -> pool exhausted.
    // Distinguish with `has_any` so callers that expect ambient do not
    // misinterpret an exhaustion as "no credentials configured".
    let mut credential = credentials::pick_for_worker(pool, caller_bucket)
        .await
        .map_err(|e| CcError::Other(anyhow::anyhow!("pick_for_worker failed: {e}")))?;
    let ambient = credential.is_none()
        && !credentials::has_any(pool)
            .await
            .map_err(|e| CcError::Other(anyhow::anyhow!("has_any failed: {e}")))?;
    if credential.is_none() && !ambient {
        // Pool configured but every credential cooling down at dispatch
        // time — surface exhaustion so the caller can set paused_until
        // instead of bouncing straight into ambient auth.
        return Err(exhausted_error(pool, caller).await);
    }

    let mut resume_session_id: Option<String> = None;
    let mut failover_count: u32 = 0;

    loop {
        let ctx = FailoverContext {
            credential: credential.clone(),
            resume_session_id: resume_session_id.clone(),
        };
        let config = build_config(ctx);

        match run_with_transient_retries(prompt, config).await {
            Ok(result) => {
                if resume_session_id.is_some() {
                    info!(
                        module = "cc-failover",
                        caller,
                        session_id = %result.session_id,
                        failover_count,
                        "recovered after credential failover"
                    );
                }
                return Ok(result);
            }
            Err(err) => {
                // 429 (rate limit) and 401 (token expired/revoked) both
                // rotate to the next healthy credential; every other
                // error (400s, 5xx after transient-retry-exhausted, spawn
                // failure, timeout, stream closed) bubbles unchanged.
                let rotate_status = match &err {
                    CcError::ApiError {
                        api_error_status: Some(status),
                        ..
                    } => matches!(status, 429 | 401),
                    _ => false,
                };
                if !rotate_status || ambient {
                    return Err(err);
                }

                let (failed_cid, failed_sid, failed_status) = match &err {
                    CcError::ApiError {
                        credential_id: Some(cid),
                        session_id,
                        api_error_status: Some(status),
                        ..
                    } => (*cid, session_id.clone(), *status),
                    _ => return Err(err),
                };
                // Exclude the failing credential from the next pick:
                //   429 -> rate-limit cooldown (temporary, recovers when
                //          window resets),
                //   401 -> mark expired (permanent until user re-auths).
                if failed_status == 429 {
                    // `resets_at`/`rate_limit_type` are observed in the
                    // stream by `session.rs` and logged, but not plumbed
                    // into the error variant. `compute_cooldown_until`
                    // with None applies the 10-minute default; the
                    // usage-poll tick refines it to the authoritative
                    // reset time on its next sweep.
                    rate_limit_activate(pool, failed_cid, None, None).await;
                } else {
                    // 401. `mark_expired` stamps `expires_at=now-1ms` so
                    // `pick_for_worker`'s `expires_at > now_ms` filter
                    // excludes this credential until the user re-auths.
                    if let Err(e) = credentials::mark_expired(pool, failed_cid).await {
                        tracing::warn!(
                            module = "cc-failover",
                            credential_id = failed_cid,
                            error = %e,
                            "failed to mark 401 credential expired"
                        );
                    } else {
                        warn!(
                            module = "cc-failover",
                            credential_id = failed_cid,
                            "credential returned 401 — marked expired"
                        );
                    }
                }

                // Pick a new credential. The just-cooled-down one is now
                // excluded by `pick_for_worker`'s filter. Done BEFORE the
                // MAX_FAILOVERS cap check so pools larger than the cap
                // still surface `AllCredentialsExhausted` once the last
                // healthy credential returns None — otherwise callers see
                // a raw 429 and fail to park the task.
                credential = credentials::pick_for_worker(pool, caller_bucket)
                    .await
                    .map_err(|e| CcError::Other(anyhow::anyhow!("pick_for_worker failed: {e}")))?;

                if credential.is_none() {
                    return Err(exhausted_error(pool, caller).await);
                }

                if failover_count >= MAX_FAILOVERS {
                    warn!(
                        module = "cc-failover",
                        caller, failover_count, "hit MAX_FAILOVERS — surfacing last error"
                    );
                    return Err(err);
                }

                resume_session_id = Some(failed_sid.clone());
                failover_count += 1;
                info!(
                    module = "cc-failover",
                    caller,
                    failover_count,
                    failed_credential_id = failed_cid,
                    resume_session_id = %failed_sid,
                    new_credential_id = credential.as_ref().map(|(id, _)| *id),
                    "failing over to next credential and resuming"
                );
            }
        }
    }
}

/// Collapse a specific caller string (e.g. `planning-cc-r1`,
/// `scout-research`) into the coarse bucket `pick_for_worker` expects
/// on its `caller_filter`. `cc_sessions.caller` values are specific;
/// the picker filters by exact match, so concurrent sibling callers
/// must share a bucket to count against the same credential's active
/// sessions for load balancing. `None` disables caller-bucket filtering
/// when a caller does not fit any bucket.
fn caller_to_bucket(caller: &str) -> Option<&'static str> {
    if caller == "worker" || caller.starts_with("worker-") {
        Some("worker")
    } else if caller == "clarifier" {
        Some("clarifier")
    } else if caller.starts_with("captain-review") {
        Some("captain-review")
    } else if caller.starts_with("captain-merge") {
        Some("captain-merge")
    } else if caller.starts_with("planning-") {
        Some("planning")
    } else if caller.starts_with("scout-") {
        Some("scout")
    } else {
        // Unknown caller: no bucket filter, count ALL active sessions
        // toward load balancing. Safer than letting the specific caller
        // string silently bypass cross-caller balance.
        None
    }
}

/// Build `CcError::AllCredentialsExhausted` from the pool's current
/// cooldown state. On DB error falls back to `FALLBACK_PARK_SECS` so the
/// task parks for a conservative window rather than re-dispatching
/// immediately (a zero default would set `paused_until = now`, which
/// the dispatch filter treats as eligible).
async fn exhausted_error(pool: &SqlitePool, caller: &str) -> CcError {
    let earliest = credentials::earliest_cooldown_remaining_secs(pool)
        .await
        .unwrap_or(FALLBACK_PARK_SECS);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let earliest_reset = now + earliest;
    warn!(
        module = "cc-failover",
        caller,
        earliest_reset,
        earliest_in_secs = earliest,
        "all credentials rate-limited"
    );
    CcError::AllCredentialsExhausted { earliest_reset }
}

/// Retry transient upstream errors (502/503/504/529) with exponential
/// back-off. 429 is Fatal (classified in `CcError::classify`) and does
/// not enter this retry loop — the outer failover loop handles it.
///
/// Each retry clears `session_id` so CC mints a fresh UUID per attempt
/// (avoids the on-disk "already in use" bail at CC startup that would
/// otherwise make the retry useless).
async fn run_with_transient_retries(
    prompt: &str,
    config: CcConfig,
) -> Result<CcResult<serde_json::Value>, CcError> {
    let mut attempt: u32 = 0;
    loop {
        let mut per_attempt = config.clone();
        if attempt > 0 {
            per_attempt.session_id = None;
        }
        match CcOneShot::run(prompt, per_attempt).await {
            Ok(result) => return Ok(result),
            Err(err) => {
                if err.classify() != ErrorClass::Transient || attempt >= MAX_TRANSIENT_RETRIES {
                    return Err(err);
                }
                let delay_ms = (500u64 << attempt).min(30_000);
                warn!(
                    module = "cc-failover",
                    caller = %config.caller,
                    attempt = attempt + 1,
                    max_retries = MAX_TRANSIENT_RETRIES,
                    delay_ms,
                    error = %err,
                    "transient — retrying with fresh session id"
                );
                attempt += 1;
                tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_700_000_000;

    #[test]
    fn compute_five_hour_uses_resets_at_directly() {
        let resets_at = NOW + 3 * 3600;
        assert_eq!(
            compute_cooldown_until(NOW, Some(resets_at), Some("five_hour")),
            resets_at + 30
        );
    }

    #[test]
    fn compute_seven_day_caps_at_one_hour() {
        let resets_at = NOW + 33 * 3600;
        assert_eq!(
            compute_cooldown_until(NOW, Some(resets_at), Some("seven_day")),
            NOW + LONG_WINDOW_MAX_COOLDOWN_SECS
        );
    }

    #[test]
    fn compute_seven_day_short_reset_not_capped() {
        let resets_at = NOW + 1200;
        assert_eq!(
            compute_cooldown_until(NOW, Some(resets_at), Some("seven_day")),
            NOW + 1200 + 30
        );
    }

    #[test]
    fn compute_unknown_type_caps_like_seven_day() {
        let resets_at = NOW + 10 * 3600;
        assert_eq!(
            compute_cooldown_until(NOW, Some(resets_at), None),
            NOW + LONG_WINDOW_MAX_COOLDOWN_SECS
        );
    }

    #[test]
    fn compute_past_resets_uses_default() {
        assert_eq!(
            compute_cooldown_until(NOW, Some(NOW - 100), Some("five_hour")),
            NOW + 600
        );
    }

    #[test]
    fn compute_none_resets_uses_default() {
        assert_eq!(
            compute_cooldown_until(NOW, None, Some("five_hour")),
            NOW + 600
        );
    }

    /// End-to-end check on the credential side of the failover wrapper:
    /// two healthy credentials, one rate_limit_activate'd, and
    /// pick_for_worker must return the *other* one. Then activate the
    /// second — `pick_for_worker` must return `None` and
    /// `earliest_cooldown_remaining_secs` must be positive. This is the
    /// deterministic core of the in-flight 429 → next-credential path;
    /// the CC subprocess half is covered separately by the classifier
    /// tests in `global-claude::error`.
    #[tokio::test]
    async fn failover_skips_cooled_down_credential_and_surfaces_exhaustion() {
        let db = global_db::Db::open_in_memory()
            .await
            .expect("in-memory db must init");
        let pool = db.pool().clone();

        let id1 = credentials::insert(&pool, "primary", "tok1", None)
            .await
            .unwrap();
        let id2 = credentials::insert(&pool, "secondary", "tok2", None)
            .await
            .unwrap();

        // Both healthy — picker honours load-balance order (fewest active
        // sessions, lowest utilisation, then id). With clean state both
        // tie, so the lowest id wins.
        let first = credentials::pick_for_worker(&pool, Some("worker"))
            .await
            .unwrap()
            .expect("at least one credential must be eligible");
        assert_eq!(first.0, id1);

        // Rate-limit credential 1 — same code path the failover wrapper
        // invokes on ApiError(429). Next pick must skip to credential 2.
        rate_limit_activate(&pool, id1, None, None).await;
        let second = credentials::pick_for_worker(&pool, Some("worker"))
            .await
            .unwrap()
            .expect("healthy credential remaining");
        assert_eq!(
            second.0, id2,
            "picker must route around the cooled-down credential"
        );

        // Rate-limit credential 2 as well — pool is now exhausted.
        rate_limit_activate(&pool, id2, None, None).await;
        let none = credentials::pick_for_worker(&pool, Some("worker"))
            .await
            .unwrap();
        assert!(
            none.is_none(),
            "picker must return None once every credential is cooling down"
        );

        let remaining = credentials::earliest_cooldown_remaining_secs(&pool)
            .await
            .unwrap();
        assert!(
            remaining > 0,
            "earliest cooldown must be in the future so callers can park tasks with paused_until"
        );
    }

    /// Clearing a cooldown puts the credential back in the eligible set
    /// immediately — matches the recovery path where a probe tick
    /// confirms the account is healthy again before its scheduled reset.
    #[tokio::test]
    async fn rate_limit_clear_restores_pick_eligibility() {
        let db = global_db::Db::open_in_memory()
            .await
            .expect("in-memory db must init");
        let pool = db.pool().clone();
        let id = credentials::insert(&pool, "only", "tok", None)
            .await
            .unwrap();

        rate_limit_activate(&pool, id, None, None).await;
        assert!(credentials::pick_for_worker(&pool, Some("worker"))
            .await
            .unwrap()
            .is_none());

        rate_limit_clear(&pool, id).await;
        let back = credentials::pick_for_worker(&pool, Some("worker"))
            .await
            .unwrap()
            .expect("credential must be eligible after clear");
        assert_eq!(back.0, id);
    }
}
