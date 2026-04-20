//! Per-credential rate-limit cooldown.
//!
//! When a session using a specific credential hits a rate limit, that
//! credential is marked with `rate_limit_cooldown_until` in the DB.
//! `pick_for_worker` already filters these out, so healthy credentials
//! keep being selected while rate-limited ones cool down.
//!
//! Cooldown duration depends on the rate-limit window type:
//! - `five_hour`: use `resets_at` directly (at most ~5h, accurate proxy).
//! - `seven_day` / other long windows: `resets_at` is the window boundary
//!   which can be days away, but the API often recovers much sooner as old
//!   usage ages out of the sliding window. Use a fixed 1-hour cooldown and
//!   let the next tick re-probe.

use sqlx::SqlitePool;

/// Maximum cooldown for long-window rate limits (seven_day, etc.) where
/// `resets_at` is a window boundary, not actual recovery time.
const LONG_WINDOW_MAX_COOLDOWN_SECS: u64 = 3600; // 1 hour

/// Compute cooldown deadline (unix seconds) from rate-limit parameters.
///
/// Extracted for testability — `activate` calls this then writes to DB.
fn compute_cooldown_until(now: u64, resets_at: Option<u64>, rate_limit_type: Option<&str>) -> u64 {
    match resets_at {
        Some(ts) if ts > now => {
            let is_long_window = !matches!(rate_limit_type, Some("five_hour"));
            if is_long_window {
                // seven_day / unknown: cap cooldown — API recovers before window boundary.
                now + (ts - now + 30).min(LONG_WINDOW_MAX_COOLDOWN_SECS)
            } else {
                // five_hour: resets_at is at most ~5h out, use it directly.
                ts + 30
            }
        }
        _ => now + 600, // 10-minute default
    }
}

/// Mark a credential as rate-limited.
///
/// `rate_limit_type` is the CC `rateLimitType` string (e.g. `"five_hour"`,
/// `"seven_day"`). For five-hour limits `resets_at` is a tight upper bound;
/// for seven-day limits it can be days away so we cap the cooldown.
#[tracing::instrument(skip_all)]
pub async fn activate(
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

    match settings::credentials::set_rate_limit_cooldown(pool, credential_id, until as i64).await {
        Ok(_) => {
            tracing::warn!(
                module = "captain",
                credential_id,
                until_epoch = until,
                cooldown_secs = until - now,
                rate_limit_type = rate_limit_type.unwrap_or("unknown"),
                "credential rate-limited"
            );
        }
        Err(e) => {
            tracing::error!(
                module = "captain",
                credential_id,
                error = %e,
                "failed to set credential rate limit cooldown"
            );
        }
    }
}

/// Clear rate-limit cooldown for a credential (recovery).
#[tracing::instrument(skip_all)]
pub async fn clear(pool: &SqlitePool, credential_id: i64) {
    match settings::credentials::set_rate_limit_cooldown(pool, credential_id, 0).await {
        Ok(_) => {
            tracing::info!(
                module = "captain",
                credential_id,
                "credential rate limit cleared (recovery)"
            );
        }
        Err(e) => {
            tracing::error!(
                module = "captain",
                credential_id,
                error = %e,
                "failed to clear credential rate limit cooldown"
            );
        }
    }
}

/// Check a session's stream for rate limit rejection and activate the
/// appropriate cooldown (per-credential or ambient).
///
/// Looks up `credential_id` from the session row to route correctly.
/// Returns `true` if a rejection was found.
#[tracing::instrument(skip_all)]
pub async fn check_and_activate_from_stream(pool: &SqlitePool, session_id: &str) -> bool {
    let stream_path = global_infra::paths::stream_path_for_session(session_id);
    match global_claude::has_rate_limit_rejection(&stream_path) {
        Some(rej) => {
            let resets = if rej.resets_at > 0 {
                Some(rej.resets_at)
            } else {
                None
            };
            let rl_type = rej.rate_limit_type.as_deref();
            let cred_id = sessions_db::get_credential_id(pool, session_id)
                .await
                .unwrap_or(None);
            if let Some(cid) = cred_id {
                activate(pool, cid, resets, rl_type).await;
            } else {
                super::ambient_rate_limit::activate(resets);
            }
            true
        }
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: u64 = 1_700_000_000;

    #[test]
    fn five_hour_uses_resets_at_directly() {
        let resets_at = NOW + 3 * 3600; // 3h from now
        let until = compute_cooldown_until(NOW, Some(resets_at), Some("five_hour"));
        assert_eq!(until, resets_at + 30);
    }

    #[test]
    fn seven_day_caps_at_one_hour() {
        let resets_at = NOW + 33 * 3600; // 33h from now (typical seven_day)
        let until = compute_cooldown_until(NOW, Some(resets_at), Some("seven_day"));
        assert_eq!(until, NOW + LONG_WINDOW_MAX_COOLDOWN_SECS);
    }

    #[test]
    fn seven_day_short_resets_at_not_capped() {
        // If seven_day resetsAt is only 20 min away, use it (under the 1h cap).
        let resets_at = NOW + 1200; // 20 min
        let until = compute_cooldown_until(NOW, Some(resets_at), Some("seven_day"));
        assert_eq!(until, NOW + 1200 + 30);
    }

    #[test]
    fn unknown_type_caps_like_seven_day() {
        let resets_at = NOW + 10 * 3600;
        let until = compute_cooldown_until(NOW, Some(resets_at), None);
        assert_eq!(until, NOW + LONG_WINDOW_MAX_COOLDOWN_SECS);
    }

    #[test]
    fn past_resets_at_uses_default() {
        let until = compute_cooldown_until(NOW, Some(NOW - 100), Some("five_hour"));
        assert_eq!(until, NOW + 600);
    }

    #[test]
    fn no_resets_at_uses_default() {
        let until = compute_cooldown_until(NOW, None, Some("five_hour"));
        assert_eq!(until, NOW + 600);
    }
}
