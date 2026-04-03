//! mando-types — shared domain types for the Mando project.

pub mod ask_history;
pub mod captain;
pub mod events;
pub mod notify;
pub mod rebase_state;
pub mod scout;
pub mod session;
pub mod session_ids;
pub mod task;
pub mod task_status;
pub mod task_update;
pub mod timeline;

// Re-exports for convenience.
pub use ask_history::AskHistoryEntry;
pub use captain::{Action, ActionKind, TickMode, TickResult, WorkerContext};
pub use events::{BusEvent, NotificationKind, NotificationPayload};
pub use notify::NotifyLevel;
pub use rebase_state::{RebaseState, RebaseStatus};
pub use scout::{ScoutItem, ScoutStatus};
pub use session::{SessionEntry, SessionStatus};
pub use session_ids::SessionIds;
pub use task::{ItemStatus, ReviewTrigger, Task, TaskRouting, TaskUpdateError};
pub use timeline::{TimelineEvent, TimelineEventType};

/// Current UTC time as an RFC 3339 string.
pub fn now_rfc3339() -> String {
    time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .expect("UTC RFC 3339 format is infallible")
}

// ── Shared path helpers ──────────────────────────────────────────────

/// User home directory via `$HOME`. Panics if `$HOME` is unset.
pub fn home_dir() -> std::path::PathBuf {
    std::env::var("HOME")
        .map(std::path::PathBuf::from)
        .expect("$HOME environment variable must be set")
}

/// Expand a leading `~` to the user's home directory.
pub fn expand_tilde(p: &str) -> std::path::PathBuf {
    if let Some(rest) = p.strip_prefix("~/") {
        home_dir().join(rest)
    } else if p == "~" {
        home_dir()
    } else {
        std::path::PathBuf::from(p)
    }
}

/// Mando data directory (`~/.mando` or `MANDO_DATA_DIR`).
pub fn data_dir() -> std::path::PathBuf {
    if let Ok(v) = std::env::var("MANDO_DATA_DIR") {
        return expand_tilde(&v);
    }
    home_dir().join(".mando")
}

#[cfg(test)]
mod tests;
