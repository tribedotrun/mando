//! Rebase state — tracks the rebase sub-state-machine for a task.

use serde::{Deserialize, Serialize};

/// Rebase state for a task, stored in the `task_rebase_state` table.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RebaseState {
    pub task_id: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worker: Option<String>,
    #[serde(default)]
    pub status: RebaseStatus,
    #[serde(default)]
    pub retries: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head_sha: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RebaseStatus {
    #[default]
    Pending,
    InProgress,
    Failed,
    Succeeded,
}

impl RebaseStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::InProgress => "in_progress",
            Self::Failed => "failed",
            Self::Succeeded => "succeeded",
        }
    }
}

impl std::str::FromStr for RebaseStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(Self::Pending),
            "in_progress" => Ok(Self::InProgress),
            "failed" => Ok(Self::Failed),
            "succeeded" => Ok(Self::Succeeded),
            other => Err(format!("unknown rebase status: {other}")),
        }
    }
}
