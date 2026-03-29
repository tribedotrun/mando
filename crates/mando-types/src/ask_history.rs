//! Ask history types for task Q&A exchanges.

use serde::{Deserialize, Serialize};

/// One Q&A exchange in a task's ask history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AskHistoryEntry {
    /// "system", "human", or "clarifier"
    pub role: String,
    pub content: String,
    /// ISO 8601 timestamp.
    pub timestamp: String,
}
