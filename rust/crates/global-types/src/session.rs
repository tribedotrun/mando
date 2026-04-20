//! CC session log entry types.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Status of a CC session (3 states).
///
/// Running ↔ Stopped/Failed — bidirectional for worker sessions (nudge/reopen
/// can resume a stopped session), one-way for all other callers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SessionStatus {
    /// CC process is alive and working.
    #[serde(rename = "running")]
    Running,
    /// CC process exited cleanly. Can be resumed via nudge/reopen/restart
    /// (worker sessions only).
    #[serde(rename = "stopped")]
    Stopped,
    /// CC process errored or crashed. Can be resumed via nudge/reopen/restart
    /// (worker sessions only).
    #[serde(rename = "failed")]
    Failed,
}

impl SessionStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Stopped => "stopped",
            Self::Failed => "failed",
        }
    }
}

impl fmt::Display for SessionStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Error returned when parsing a `SessionStatus` from a string fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseSessionStatusError {
    /// The input value that failed to parse.
    pub value: String,
}

impl fmt::Display for ParseSessionStatusError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown session status: {}", self.value)
    }
}

impl std::error::Error for ParseSessionStatusError {}

impl FromStr for SessionStatus {
    type Err = ParseSessionStatusError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "running" => Ok(Self::Running),
            "stopped" => Ok(Self::Stopped),
            "failed" => Ok(Self::Failed),
            _ => Err(ParseSessionStatusError {
                value: s.to_string(),
            }),
        }
    }
}

/// A single CC session log entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SessionEntry {
    pub session_id: String,
    pub ts: String,
    pub cwd: String,
    pub model: String,
    pub caller: String,
    pub resumed: bool,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<i64>,
    pub title: String,
    pub project: String,
    pub task_id: String,
    pub worker_name: String,
    pub status: String,
}

impl Default for SessionEntry {
    fn default() -> Self {
        Self {
            session_id: String::new(),
            ts: String::new(),
            cwd: String::new(),
            model: String::new(),
            caller: String::new(),
            resumed: false,
            source: "live".into(),
            cost_usd: None,
            duration_ms: None,
            title: String::new(),
            project: String::new(),
            task_id: String::new(),
            worker_name: String::new(),
            status: "stopped".into(),
        }
    }
}
