use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::MandoConfig;

/// POST /api/tasks/ask
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(optional_fields)]
#[serde(deny_unknown_fields)]
pub struct TaskAskRequest {
    pub id: i64,
    pub question: String,
    pub ask_id: Option<String>,
}

/// PATCH /api/tasks/{id}. `skip_serializing_if` on every Option is required:
/// captain's `apply_json_updates` treats a JSON `null` as "clear this field",
/// so unset fields must disappear from the serialized payload.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(optional_fields)]
#[serde(deny_unknown_fields)]
pub struct TaskPatchRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub original_prompt: Option<String>,
    /// Manual override for the clarifier's bug-fix classification. Sent from
    /// the task editor when the user disagrees with the auto-classified value
    /// (or wants to correct it after a misread). The captain workflow reads
    /// this on the next worker spawn and captain review tick.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_bug_fix: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ClarifyAnswer {
    pub question: String,
    pub answer: String,
}

/// POST /api/tasks/{id}/clarify
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(optional_fields)]
#[serde(deny_unknown_fields)]
pub struct ClarifyRequest {
    pub answers: Option<Vec<ClarifyAnswer>>,
    pub answer: Option<String>,
}

/// POST /api/tasks/{id}/clarify query params.
///
/// `wait = Some(false)` makes the route return as soon as the answer is
/// committed and the follow-up CC reclarify call is spawned; the result
/// arrives via SSE. Default (`None` / `Some(true)`) preserves the
/// synchronous response that CLI / Telegram callers rely on.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ClarifyQuery {
    pub wait: Option<bool>,
}

/// GET /api/scout/items query params
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ScoutQuery {
    pub status: Option<ScoutItemStatusFilter>,
    pub q: Option<String>,
    #[serde(rename = "type")]
    pub item_type: Option<String>,
    pub page: Option<usize>,
    pub per_page: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ScoutItemStatusFilter {
    All,
    Pending,
    Fetched,
    Processed,
    Saved,
    Archived,
    Error,
}

impl ScoutItemStatusFilter {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Pending => "pending",
            Self::Fetched => "fetched",
            Self::Processed => "processed",
            Self::Saved => "saved",
            Self::Archived => "archived",
            Self::Error => "error",
        }
    }
}

/// POST /api/scout/ask
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(optional_fields)]
#[serde(deny_unknown_fields)]
pub struct ScoutAskRequest {
    pub id: i64,
    pub question: String,
    pub session_id: Option<String>,
}

/// POST /api/scout/items/{id}/act
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(optional_fields)]
#[serde(deny_unknown_fields)]
pub struct ScoutActRequest {
    pub project: String,
    pub prompt: Option<String>,
}

/// POST /api/scout/process
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(optional_fields)]
#[serde(deny_unknown_fields)]
pub struct ScoutProcessRequest {
    pub id: Option<i64>,
}

/// POST /api/scout/research
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(optional_fields)]
#[serde(deny_unknown_fields)]
pub struct ScoutResearchRequest {
    pub topic: String,
    pub process: Option<bool>,
}

/// Command surface for scout item lifecycle transitions.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ScoutItemLifecycleCommand {
    MarkPending,
    MarkProcessed,
    Save,
    Archive,
}

/// POST /api/scout/bulk
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ScoutBulkCommandRequest {
    pub ids: Vec<i64>,
    pub command: ScoutItemLifecycleCommand,
}

/// POST /api/scout/bulk-delete
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ScoutBulkDeleteRequest {
    pub ids: Vec<i64>,
}

/// POST /api/projects/add
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(optional_fields)]
#[serde(deny_unknown_fields)]
pub struct AddProjectRequest {
    pub name: Option<String>,
    pub path: String,
    pub aliases: Vec<String>,
}

/// PATCH /api/projects/{key}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(optional_fields)]
#[serde(deny_unknown_fields)]
pub struct EditProjectRequest {
    pub rename: Option<String>,
    pub github_repo: Option<String>,
    pub clear_github_repo: Option<bool>,
    pub aliases: Option<Vec<String>>,
    pub hooks: Option<std::collections::HashMap<String, String>>,
    pub preamble: Option<String>,
    pub check_command: Option<String>,
    pub scout_summary: Option<String>,
    pub redetect_logo: Option<bool>,
}

/// POST /api/scout/items
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(optional_fields)]
#[serde(deny_unknown_fields)]
pub struct ScoutAddRequest {
    pub url: String,
    pub title: Option<String>,
}

/// PATCH /api/scout/items/{id}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ScoutLifecycleCommandRequest {
    pub command: ScoutItemLifecycleCommand,
}

/// POST /api/captain/adopt
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(optional_fields)]
#[serde(deny_unknown_fields)]
pub struct AdoptRequest {
    pub title: String,
    pub worktree_path: String,
    pub note: Option<String>,
    pub project: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum WorkbenchStatusFilter {
    Active,
    Archived,
    All,
}

impl WorkbenchStatusFilter {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Archived => "archived",
            Self::All => "all",
        }
    }
}

/// GET /api/workbenches query params
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct WorkbenchListQuery {
    pub status: Option<WorkbenchStatusFilter>,
}

/// PATCH /api/workbenches/{id}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(optional_fields)]
#[serde(deny_unknown_fields)]
pub struct WorkbenchPatchRequest {
    pub title: Option<String>,
    pub archived: Option<bool>,
    pub pinned: Option<bool>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct TerminalSize {
    pub rows: u16,
    pub cols: u16,
}

/// POST /api/terminal
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(optional_fields)]
#[serde(deny_unknown_fields)]
pub struct TerminalCreateRequest {
    pub project: String,
    pub cwd: String,
    pub agent: crate::TerminalAgent,
    pub resume_session_id: Option<String>,
    pub size: Option<TerminalSize>,
    pub terminal_id: Option<String>,
    pub name: Option<String>,
}

/// POST /api/terminal/{id}/write
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct TerminalWriteRequest {
    pub data: String,
}

/// GET /api/terminal/{id}/stream
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct TerminalStreamQuery {
    pub replay: Option<u8>,
}

/// POST /api/terminal/{id}/cc-session
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TerminalCcSessionRequest {
    pub cc_session_id: String,
}

/// POST /api/worktrees
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(optional_fields)]
#[serde(deny_unknown_fields)]
pub struct CreateWorktreeRequest {
    pub name: Option<String>,
    pub project: Option<String>,
}

/// POST /api/worktrees/remove
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct RemoveWorktreeRequest {
    pub path: String,
}

/// POST /api/credentials/setup-token
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SetupTokenRequest {
    pub label: String,
    pub token: String,
}

/// POST /api/config/setup
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(optional_fields)]
#[serde(deny_unknown_fields)]
pub struct ConfigSetupRequest {
    pub config: Option<MandoConfig>,
}

/// POST /api/tasks/{id}/advisor
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct AdvisorRequest {
    pub message: String,
    pub intent: String,
}

/// POST /api/ui/register
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UiRegisterRequest {
    pub pid: i32,
    pub exec_path: String,
    pub args: Vec<String>,
    pub cwd: Option<String>,
    pub env: std::collections::HashMap<String, String>,
}

/// POST /api/client-logs
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClientLogEntry {
    pub level: String,
    pub message: String,
    pub context: Option<crate::ClientLogContext>,
    pub timestamp: Option<String>,
}

/// POST /api/client-logs
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ClientLogBatchRequest {
    pub entries: Vec<ClientLogEntry>,
}
