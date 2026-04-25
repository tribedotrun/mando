use std::time::{Duration, Instant};

use crate::config::{DEGRADED_FAILURE_COUNT, MAX_BACKOFF_SECS};

pub(crate) fn telegram_enabled(config: &settings::Config) -> bool {
    config.channels.telegram.enabled && !config.channels.telegram.token.is_empty()
}

pub(crate) fn degraded_window_exhausted(
    failure_count: u32,
    first_failure_at: Option<Instant>,
    now: Instant,
    window: Duration,
) -> bool {
    failure_count >= DEGRADED_FAILURE_COUNT
        && first_failure_at.is_some_and(|first| now.duration_since(first) < window)
}

pub(crate) fn restart_backoff_secs(failure_count: u32) -> u64 {
    (1u64 << (failure_count - 1).min(6)).min(MAX_BACKOFF_SECS)
}
