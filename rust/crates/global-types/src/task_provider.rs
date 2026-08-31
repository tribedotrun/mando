use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Provider allowed to own a persisted task.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum TaskOwnerProvider {
    #[default]
    Claude,
    Codex,
}

impl TaskOwnerProvider {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Claude => "Claude Code",
            Self::Codex => "Codex",
        }
    }
}

impl fmt::Display for TaskOwnerProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for TaskOwnerProvider {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "claude" => Ok(Self::Claude),
            "codex" => Ok(Self::Codex),
            _ => Err(format!("unknown task owner provider: {s}")),
        }
    }
}

/// Adapter that executes an agent session.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(rename = "TaskProvider")]
pub enum ExecutionAdapter {
    #[default]
    Claude,
    Codex,
    OpenCode,
}

impl ExecutionAdapter {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::OpenCode => "opencode",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Claude => "Claude Code",
            Self::Codex => "Codex",
            Self::OpenCode => "OpenCode",
        }
    }
}

impl fmt::Display for ExecutionAdapter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ExecutionAdapter {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "claude" => Ok(Self::Claude),
            "codex" => Ok(Self::Codex),
            "opencode" => Ok(Self::OpenCode),
            _ => Err(format!("unknown execution adapter: {s}")),
        }
    }
}

impl From<TaskOwnerProvider> for ExecutionAdapter {
    fn from(provider: TaskOwnerProvider) -> Self {
        match provider {
            TaskOwnerProvider::Claude => Self::Claude,
            TaskOwnerProvider::Codex => Self::Codex,
        }
    }
}

impl TryFrom<ExecutionAdapter> for TaskOwnerProvider {
    type Error = String;

    fn try_from(adapter: ExecutionAdapter) -> Result<Self, Self::Error> {
        match adapter {
            ExecutionAdapter::Claude => Ok(Self::Claude),
            ExecutionAdapter::Codex => Ok(Self::Codex),
            ExecutionAdapter::OpenCode => Err("OpenCode cannot own a persisted task".to_string()),
        }
    }
}

/// Compatibility name for existing wire consumers.
pub type TaskProvider = ExecutionAdapter;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_owner_rejects_opencode() {
        assert!("opencode".parse::<TaskOwnerProvider>().is_err());
        assert!(TaskOwnerProvider::try_from(ExecutionAdapter::OpenCode).is_err());
        assert!(TaskOwnerProvider::try_from(TaskProvider::OpenCode).is_err());
    }

    #[test]
    fn task_owner_maps_to_execution_adapter() {
        assert_eq!(
            ExecutionAdapter::from(TaskOwnerProvider::Claude),
            ExecutionAdapter::Claude
        );
        assert_eq!(
            ExecutionAdapter::from(TaskOwnerProvider::Codex),
            ExecutionAdapter::Codex
        );
    }
}
