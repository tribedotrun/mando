use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub type SessionId = String;

#[derive(Debug, Clone)]
pub enum TerminalEvent {
    Output(Vec<u8>),
    Exit { code: Option<u32> },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct TerminalSize {
    pub rows: u16,
    pub cols: u16,
}

impl Default for TerminalSize {
    fn default() -> Self {
        Self { rows: 24, cols: 80 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Agent {
    Claude,
    Codex,
}

impl std::fmt::Display for Agent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Agent::Claude => write!(f, "claude"),
            Agent::Codex => write!(f, "codex"),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateRequest {
    pub project: String,
    pub cwd: PathBuf,
    pub agent: Agent,
    #[serde(default)]
    pub resume_session_id: Option<String>,
    #[serde(default)]
    pub size: Option<TerminalSize>,
    /// Extra environment variables injected into the PTY process.
    #[serde(default)]
    pub extra_env: std::collections::HashMap<String, String>,
    /// Extra CLI arguments parsed from config (shell-split).
    #[serde(default)]
    pub extra_args: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionInfo {
    pub id: SessionId,
    pub project: String,
    pub cwd: PathBuf,
    pub agent: Agent,
    pub running: bool,
    pub exit_code: Option<u32>,
    pub rev: u64,
}
