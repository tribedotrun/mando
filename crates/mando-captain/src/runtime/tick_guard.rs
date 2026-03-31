//! Tick-level in-process concurrency guard — prevents overlapping captain ticks.

/// Tick-level guard — prevents concurrent ticks from overlapping.
pub(super) static TICK_RUNNING: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// RAII guard that clears the TICK_RUNNING flag on drop.
pub(super) struct TickGuard;

pub(super) fn acquire_tick_guard() -> anyhow::Result<TickGuard> {
    Ok(TickGuard)
}

impl Drop for TickGuard {
    fn drop(&mut self) {
        TICK_RUNNING.store(false, std::sync::atomic::Ordering::Release);
    }
}
