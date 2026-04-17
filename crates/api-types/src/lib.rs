//! api-types -- HTTP request/response contract for the Mando daemon API.
//!
//! Shared between transport-http (server) and mando-cli (client).
//! All types derive Serialize + Deserialize. When ts-rs is added,
//! they will also derive TS for automatic TypeScript generation.

use serde::{Deserialize, Serialize};

/// Standard JSON error envelope returned by all error responses.
#[derive(Debug, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub error: String,
}

/// POST /api/tasks/add
#[derive(Debug, Deserialize)]
pub struct TaskAddRequest {
    pub title: String,
    #[serde(default)]
    pub project: Option<String>,
    #[serde(default)]
    pub plan: bool,
    #[serde(default)]
    pub no_pr: bool,
}

/// POST /api/captain/tick
#[derive(Debug, Deserialize)]
pub struct TickRequest {
    #[serde(default)]
    pub dry_run: bool,
}

/// POST /api/captain/triage
#[derive(Debug, Deserialize)]
pub struct TriageRequest {
    #[serde(default)]
    pub item_id: Option<String>,
}

/// POST /api/tasks/reopen, /api/tasks/rework
#[derive(Debug, Deserialize)]
pub struct TaskFeedbackRequest {
    pub id: i64,
    pub feedback: String,
}

/// POST /api/tasks/retry, /api/tasks/accept, /api/tasks/handoff
#[derive(Debug, Deserialize)]
pub struct TaskIdRequest {
    pub id: i64,
}

/// POST /api/tasks/merge
#[derive(Debug, Deserialize)]
pub struct MergeRequest {
    pub pr_number: i64,
    #[serde(default)]
    pub project: String,
}

/// POST /api/captain/nudge
#[derive(Debug, Deserialize)]
pub struct NudgeRequest {
    pub id: i64,
    pub message: String,
}

/// GET /api/sessions query params
#[derive(Debug, Deserialize)]
pub struct SessionsQuery {
    #[serde(default)]
    pub task: Option<i64>,
    #[serde(default)]
    pub caller: Option<String>,
    #[serde(default)]
    pub last: Option<usize>,
}

/// POST /api/projects/add
#[derive(Debug, Deserialize)]
pub struct AddProjectRequest {
    pub name: String,
    pub path: String,
    #[serde(default)]
    pub github_repo: Option<String>,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub check_command: Option<String>,
    #[serde(default)]
    pub scout_summary: Option<String>,
    #[serde(default)]
    pub worker_preamble: Option<String>,
}

/// PATCH /api/projects/{key}
#[derive(Debug, Deserialize)]
pub struct EditProjectRequest {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub github_repo: Option<String>,
    #[serde(default)]
    pub aliases: Option<Vec<String>>,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(default)]
    pub check_command: Option<String>,
    #[serde(default)]
    pub scout_summary: Option<String>,
    #[serde(default)]
    pub worker_preamble: Option<String>,
}

/// POST /api/scout/items
#[derive(Debug, Deserialize)]
pub struct ScoutAddRequest {
    pub url: String,
    #[serde(default)]
    pub title: Option<String>,
}

/// PATCH /api/scout/items/{id}
#[derive(Debug, Deserialize)]
pub struct ScoutStatusUpdate {
    pub status: String,
}

/// POST /api/captain/adopt
#[derive(Debug, Deserialize)]
pub struct AdoptRequest {
    pub title: String,
    pub worktree_path: String,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub project: Option<String>,
}
