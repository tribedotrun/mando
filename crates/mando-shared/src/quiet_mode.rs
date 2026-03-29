//! Quiet mode — suppress LOW/NORMAL notifications during focus periods.

use std::sync::atomic::{AtomicBool, Ordering};

static QUIET_MODE: AtomicBool = AtomicBool::new(false);

/// Check if quiet mode is currently active.
pub fn is_active() -> bool {
    QUIET_MODE.load(Ordering::Relaxed)
}
