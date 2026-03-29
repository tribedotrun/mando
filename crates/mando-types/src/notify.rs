//! Notification priority levels for captain Telegram notifications.

use serde::{Deserialize, Serialize};

/// Priority level for captain notifications.
///
/// Messages below the configured threshold are logged but not sent to TG.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[repr(u8)]
pub enum NotifyLevel {
    /// Internal machinery (rebase lifecycle, deep-clarify details).
    Low = 10,
    /// Operational (spawned worker).
    Normal = 20,
    /// Major state changes (done, answered, review-reopened).
    High = 30,
    /// Human intervention needed (FAILED, exhausted, errors).
    Critical = 40,
}

impl NotifyLevel {
    /// Numeric value of this level.
    pub fn value(self) -> u8 {
        self as u8
    }
}

impl PartialOrd for NotifyLevel {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for NotifyLevel {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.value().cmp(&other.value())
    }
}
