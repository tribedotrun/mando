use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionState {
    Live,
    Restored,
    Exited,
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
    /// Extra environment variables from config.json env.
    #[serde(default)]
    pub config_env: HashMap<String, String>,
    /// Explicit terminal-scoped environment variables injected last.
    #[serde(default)]
    pub terminal_env: HashMap<String, String>,
    #[serde(default)]
    pub terminal_id: Option<String>,
    /// Extra CLI arguments parsed from config (shell-split).
    #[serde(default)]
    pub extra_args: Vec<String>,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionInfo {
    pub id: SessionId,
    pub rev: u64,
    pub project: String,
    pub cwd: PathBuf,
    pub agent: Agent,
    pub running: bool,
    pub exit_code: Option<u32>,
    pub state: SessionState,
    pub restored: bool,
    #[serde(rename = "createdAt")]
    pub created_at: String,
    #[serde(rename = "endedAt")]
    pub ended_at: Option<String>,
    #[serde(rename = "terminalId")]
    pub terminal_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "ccSessionId")]
    pub cc_session_id: Option<String>,
    /// Last-known PTY size, used to restore correct dimensions on auto-resume.
    #[serde(skip)]
    pub size: TerminalSize,
}
