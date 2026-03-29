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

impl FromStr for SessionStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "running" => Ok(Self::Running),
            "stopped" => Ok(Self::Stopped),
            // Accept legacy "done" as Stopped.
            "done" => Ok(Self::Stopped),
            "failed" => Ok(Self::Failed),
            _ => Err(format!("unknown session status: {s}")),
        }
    }
}

/// A single CC session log entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionEntry {
    pub session_id: String,
    #[serde(default)]
    pub ts: String,
    #[serde(default)]
    pub cwd: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub caller: String,
    #[serde(default)]
    pub resumed: bool,
    #[serde(default = "default_source")]
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<i64>,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub project: String,
    #[serde(default)]
    pub task_id: String,
    #[serde(default)]
    pub worker_name: String,
    #[serde(default = "default_status")]
    pub status: String,
}

fn default_source() -> String {
    "live".into()
}

fn default_status() -> String {
    "stopped".into()
}
