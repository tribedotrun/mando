use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use ts_rs::TS;

use crate::{SessionCategory, SessionStatus, TaskProvider};

/// Standard JSON error envelope returned by all error responses.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ErrorResponse {
    pub error: String,
}

/// Wire body for POST/PUT/PATCH routes that carry no payload. Serializes to
/// `{}`; rejects any fields. Named to make "no body" an explicit declaration
/// on both the macro side (`body = api_types::EmptyRequest`) and every typed
/// client caller. Never omit the `body = ` declaration.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct EmptyRequest {}

/// Wire response for routes that return no payload. Serializes to `{}`.
/// Paired with `EmptyRequest` for the request side.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct EmptyResponse {}

/// POST /api/tasks/add
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct TaskAddRequest {
    pub title: String,
    pub project: Option<String>,
    pub provider: Option<TaskCreateProvider>,
    pub use_glm_worker: bool,
    pub plan: bool,
    pub no_pr: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum TaskCreateProvider {
    Claude,
    Codex,
}

impl TaskCreateProvider {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
        }
    }

    pub fn as_task_provider(self) -> TaskProvider {
        match self {
            Self::Claude => TaskProvider::Claude,
            Self::Codex => TaskProvider::Codex,
        }
    }
}

impl fmt::Display for TaskCreateProvider {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for TaskCreateProvider {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "claude" => Ok(Self::Claude),
            "codex" => Ok(Self::Codex),
            _ => Err("task provider must be Codex or Claude Code".to_string()),
        }
    }
}

/// Patchable fields in a bulk-task update. All optional -- a field left `None`
/// is not changed on any of the targeted tasks. `skip_serializing_if` is
/// required: captain's `apply_json_updates` treats a JSON `null` as "clear
/// this field", so unset fields must disappear from the serialized payload
/// rather than serialize as `null`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(optional_fields)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TaskBulkUpdates {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub worker: Option<String>,
}

/// POST /api/captain/tick
///
/// Empty body runs one tick (existing behavior). Any of `until_idle`,
/// `max_ticks`, or `until_status` flips the handler into drain mode — it
/// loops `trigger_captain_tick` until the requested condition is met or
/// a hard cap trips (see `TickDrainResult.stopped_reason`).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(optional_fields)]
#[serde(deny_unknown_fields)]
pub struct TickRequest {
    pub dry_run: Option<bool>,
    pub emit_notifications: Option<bool>,
    /// Drain ticks until a pass reports no state changes (iterations converge),
    /// or until a cap trips.
    pub until_idle: Option<bool>,
    /// Hard upper bound on ticks in this call. Clamped to a server-side
    /// ceiling so a misbehaving caller can't peg the daemon.
    pub max_ticks: Option<u32>,
    /// Drain until the task identified by `task_id` reaches any of these
    /// statuses. Requires `task_id`; a value without `task_id` is a 400.
    pub until_status: Option<Vec<crate::ItemStatus>>,
    /// Target task for `until_status`.
    pub task_id: Option<i64>,
}

/// POST /api/captain/triage
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(optional_fields)]
#[serde(deny_unknown_fields)]
pub struct TriageRequest {
    pub item_id: Option<String>,
}

/// POST /api/tasks/reopen, /api/tasks/rework
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct TaskFeedbackRequest {
    pub id: i64,
    pub feedback: String,
}

/// POST /api/tasks/retry, /api/tasks/accept, /api/tasks/handoff
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct TaskIdRequest {
    pub id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct TaskIdParams {
    pub id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ArtifactIdParams {
    pub id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ArtifactMediaParams {
    pub id: i64,
    pub index: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct WorkbenchIdParams {
    pub id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ScoutItemIdParams {
    pub id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ScoutResearchIdParams {
    pub id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct CredentialIdParams {
    pub id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SessionIdParams {
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ProjectNameParams {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ImageFilenameParams {
    pub filename: String,
}

/// POST /api/tasks/merge
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct MergeRequest {
    pub pr_number: i64,
    pub project: String,
}

/// POST /api/captain/nudge
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct NudgeRequest {
    pub item_id: String,
    pub message: String,
}

/// GET /api/sessions query params
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SessionsQuery {
    pub page: Option<u32>,
    pub per_page: Option<u32>,
    pub category: Option<SessionCategory>,
    /// Alias for category -- accepted from CLI which sends `caller=`.
    pub caller: Option<SessionCategory>,
    pub status: Option<SessionStatus>,
}

/// GET /api/sessions/{id}/messages query params
#[derive(Debug, Clone, Serialize, Deserialize, TS, Default)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields)]
pub struct SessionMessagesQuery {
    #[ts(type = "number | null")]
    pub limit: Option<usize>,
    #[ts(type = "number | null")]
    pub offset: Option<usize>,
}

/// GET /api/sessions/{id}/stream query params
#[derive(Debug, Clone, Serialize, Deserialize, TS, Default)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields)]
pub struct SessionStreamQuery {
    pub types: Option<String>,
}

/// GET /api/tasks query params
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct TaskListQuery {
    pub include_archived: Option<bool>,
}

/// POST /api/tasks/delete
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(optional_fields)]
#[serde(deny_unknown_fields)]
pub struct TaskDeleteRequest {
    pub ids: Vec<i64>,
    pub close_pr: Option<bool>,
    pub force: Option<bool>,
}
