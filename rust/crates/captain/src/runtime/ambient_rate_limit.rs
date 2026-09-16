//! Ambient (host login) rate-limit cooldown.
//!
//! Governs sessions that use the host's Claude Code login (no credential).
//! When credentials are configured, workers use per-credential cooldowns
//! instead (see `credential_rate_limit` module and DB column
//! `credentials.rate_limit_cooldown_until`).

use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

/// Minimum cooldown when `resets_at` is unknown or in the past.
const BASE_COOLDOWN_SECS: u64 = 600; // 10 minutes
/// Maximum cooldown duration (cap for exponential backoff).
const MAX_COOLDOWN_SECS: u64 = 1800; // 30 minutes
/// Small buffer added to `resets_at`-based cooldowns.
const RESET_BUFFER_SECS: u64 = 30;

static COOLDOWN: LazyLock<Mutex<CooldownState>> =
    LazyLock::new(|| Mutex::new(CooldownState::default()));

#[derive(Debug, Default)]
struct CooldownState {
    /// When the cooldown expires. `None` = not in cooldown.
    until: Option<Instant>,
    /// Consecutive activations without a recovery (Allowed status).
    /// Drives exponential backoff when `resets_at` is unknown.
    consecutive: u32,
}

/// Lock the cooldown mutex, recovering from poison. A poisoned mutex means a
/// thread panicked while holding the lock, but the CooldownState inside is
/// still usable (it's just timestamps and a counter). Recovering avoids a
/// permanent captain freeze.
fn lock_cooldown() -> std::sync::MutexGuard<'static, CooldownState> {
    COOLDOWN.lock().unwrap_or_else(|poisoned| {
        tracing::warn!(
            module = "captain",
            "rate limit cooldown mutex was poisoned — recovering"
        );
        poisoned.into_inner()
    })
}

/// Activate cooldown. Called when a rate_limit Rejected event is observed.
///
/// - If `resets_at` is a future unix timestamp: cooldown = (resets_at - now) + buffer.
/// - Otherwise: exponential backoff based on consecutive activations.
pub fn activate(resets_at_unix: Option<u64>) {
    let mut state = lock_cooldown();

    let now_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let duration = match resets_at_unix {
        Some(ts) if ts > now_unix => {
            Duration::from_secs(((ts - now_unix) + RESET_BUFFER_SECS).min(MAX_COOLDOWN_SECS))
        }
        _ => {
            // Unknown or past — exponential backoff.
            let secs = (BASE_COOLDOWN_SECS * 2u64.saturating_pow(state.consecutive))
                .min(MAX_COOLDOWN_SECS);
            Duration::from_secs(secs)
        }
    };

    let new_until = Instant::now() + duration;

    // Only extend cooldown, never shorten it.
    let should_update = state.until.is_none_or(|existing| new_until > existing);
    if should_update {
        state.until = Some(new_until);
        state.consecutive += 1;
        tracing::warn!(
            module = "captain",
            cooldown_secs = duration.as_secs(),
            consecutive = state.consecutive,
            "rate limit cooldown activated"
        );
    }
}

/// Check if cooldown is currently active.
pub fn is_active() -> bool {
    let state = lock_cooldown();
    state.until.is_some_and(|until| Instant::now() < until)
}

/// Seconds remaining in cooldown (0 if not active).
pub fn remaining_secs() -> u64 {
    let state = lock_cooldown();
    state
        .until
        .and_then(|until| until.checked_duration_since(Instant::now()))
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Clear cooldown — called on recovery (Allowed rate limit status).
pub fn clear() {
    let mut state = lock_cooldown();
    if state.until.is_some() {
        tracing::info!(module = "captain", "rate limit cooldown cleared (recovery)");
    }
    state.until = None;
    state.consecutive = 0;
}
