//! Ask history types for task Q&A exchanges.

use serde::{Deserialize, Serialize};

/// One message in a task's ask history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AskHistoryEntry {
    /// Conversation grouping key (UUID, generated per Q&A session).
    pub ask_id: String,
    /// CC session that produced this row.
    pub session_id: String,
    /// "human", "assistant", or "error"
    pub role: String,
    pub content: String,
    /// ISO 8601 timestamp.
    pub timestamp: String,
}
