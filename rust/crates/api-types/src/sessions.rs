use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS)]
pub enum SessionStatus {
    #[serde(rename = "running")]
    Running,
    #[serde(rename = "stopped")]
    Stopped,
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

impl std::fmt::Display for SessionStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str((*self).as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
pub enum SessionCategory {
    Workers,
    Clarifier,
    CaptainReview,
    CaptainOps,
    Advisor,
    Planning,
    TodoParser,
    Scout,
    Rebase,
}

impl SessionCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Workers => "workers",
            Self::Clarifier => "clarifier",
            Self::CaptainReview => "captain-review",
            Self::CaptainOps => "captain-ops",
            Self::Advisor => "advisor",
            Self::Planning => "planning",
            Self::TodoParser => "todo-parser",
            Self::Scout => "scout",
            Self::Rebase => "rebase",
        }
    }
}

impl std::fmt::Display for SessionCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str((*self).as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SessionEntry {
    pub session_id: String,
    pub created_at: String,
    pub cwd: String,
    pub model: String,
    pub caller: String,
    pub resumed: bool,
    pub cost_usd: Option<f64>,
    pub duration_ms: Option<i64>,
    pub turn_count: Option<i64>,
    pub scout_item_id: Option<i64>,
    pub task_id: Option<String>,
    pub worker_name: Option<String>,
    pub resumed_at: Option<String>,
    pub status: SessionStatus,
    pub task_title: Option<String>,
    pub scout_item_title: Option<String>,
    pub github_repo: Option<String>,
    pub pr_number: Option<i64>,
    pub worktree: Option<String>,
    pub branch: Option<String>,
    pub resume_cwd: Option<String>,
    pub category: Option<SessionCategory>,
    pub credential_id: Option<i64>,
    pub credential_label: Option<String>,
    pub error: Option<String>,
    pub api_error_status: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SessionSummary {
    pub session_id: String,
    pub status: SessionStatus,
    pub caller: String,
    pub started_at: String,
    pub duration_ms: Option<i64>,
    pub cost_usd: Option<f64>,
    pub model: Option<String>,
    pub resumed: bool,
    pub cwd: Option<String>,
    pub worker_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct TranscriptToolCall {
    pub id: String,
    pub name: String,
    pub input_summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct TranscriptUsageInfo {
    #[ts(type = "number")]
    pub input_tokens: u64,
    #[ts(type = "number")]
    pub output_tokens: u64,
    #[ts(type = "number")]
    pub cache_read_tokens: u64,
    #[ts(type = "number")]
    pub cache_creation_tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct TranscriptMessage {
    pub role: String,
    pub uuid: String,
    pub parent_uuid: Option<String>,
    pub text: String,
    pub tool_calls: Vec<TranscriptToolCall>,
    pub usage: Option<TranscriptUsageInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SessionToolUsageSummary {
    pub name: String,
    #[ts(type = "number")]
    pub call_count: u32,
    #[ts(type = "number")]
    pub error_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SessionCostSummary {
    #[ts(type = "number")]
    pub total_input_tokens: u64,
    #[ts(type = "number")]
    pub total_output_tokens: u64,
    #[ts(type = "number")]
    pub total_cache_read_tokens: u64,
    #[ts(type = "number")]
    pub total_cache_creation_tokens: u64,
    #[ts(type = "number")]
    pub turn_count: u32,
    pub total_cost_usd: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SessionJsonlPathResponse {
    pub session_id: String,
    pub path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SessionsResponse {
    pub total: usize,
    pub page: usize,
    pub per_page: usize,
    pub total_pages: usize,
    pub categories: HashMap<String, usize>,
    pub total_cost_usd: Option<f64>,
    pub sessions: Vec<SessionEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SessionsListResponse {
    pub total: usize,
    pub page: usize,
    pub per_page: usize,
    pub total_pages: usize,
    pub categories: BTreeMap<String, u64>,
    pub total_cost_usd: f64,
    pub sessions: Vec<SessionEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SessionMessagesResponse {
    pub messages: Vec<TranscriptMessage>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SessionToolUsageResponse {
    pub tools: Vec<SessionToolUsageSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SessionCostResponse {
    pub cost: SessionCostSummary,
}
