use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde_json::Value;
use tokio::sync::mpsc;

pub type StderrTail = Arc<Mutex<VecDeque<String>>>;

pub struct StartTurnRequest {
    pub cwd: PathBuf,
    pub prompt: String,
    pub resume_thread_id: Option<String>,
    pub output_schema: Option<Value>,
    pub codex: CodexTurnConfig,
    pub sandbox: String,
    pub sandbox_policy: Value,
    pub approval_policy: String,
    pub approvals_reviewer: String,
    pub response_timeout: std::time::Duration,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CodexTurnConfig {
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
    pub service_tier: Option<String>,
}

pub struct StartedTurn {
    pub thread_id: String,
    pub turn_id: String,
    pub model: String,
    pub pid: global_types::Pid,
    pub stderr_tail: StderrTail,
    pub expects_structured_output: bool,
    pub response_timeout: std::time::Duration,
    pub events: mpsc::UnboundedReceiver<AppServerEvent>,
}

#[derive(Debug)]
pub enum AppServerEvent {
    Notification(Value),
    Fatal(String),
}
