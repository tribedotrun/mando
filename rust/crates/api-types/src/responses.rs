use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{
    AskHistoryEntry, ClarifierQuestion, DailyMerge, FeedItem, ProjectSummary, ScoutItem,
    SessionSummary, TaskArtifact, TaskItem, TimelineEvent, WorkbenchItem, WorkerDetail,
};

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct HealthResponse {
    pub healthy: bool,
    pub version: String,
    #[ts(type = "number")]
    pub pid: u32,
    #[ts(type = "number")]
    pub uptime: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct BoolOkResponse {
    pub ok: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ParseTodosResponse {
    pub items: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct TaskCreateResponse {
    pub id: i64,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct BulkFailure {
    pub id: i64,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ScoutBulkUpdateResponse {
    pub updated: u32,
    pub failed: Vec<BulkFailure>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ScoutBulkDeleteResponse {
    pub deleted: u32,
    pub failed: Vec<BulkFailure>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct TaskListResponse {
    pub items: Vec<TaskItem>,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct DeleteTasksResponse {
    pub ok: bool,
    pub deleted: usize,
    pub warnings: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ClarifyResponse {
    pub ok: bool,
    pub status: String,
    pub context: Option<String>,
    pub questions: Option<Vec<ClarifierQuestion>>,
    pub session_id: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct NudgeResponse {
    pub ok: bool,
    pub worker: Option<String>,
    pub pid: Option<u32>,
    pub status: Option<String>,
    pub alerts: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct StopWorkersResponse {
    pub killed: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct WorkersResponse {
    pub workers: Vec<WorkerDetail>,
    pub rate_limit_remaining_secs: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ActivityStatsResponse {
    pub merged_7d: i64,
    pub daily_merges: Vec<DailyMerge>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct AskResponse {
    pub id: Option<i64>,
    pub ask_id: String,
    pub question: Option<String>,
    pub answer: String,
    pub session_id: Option<String>,
    pub suggested_followups: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct AskEndResponse {
    pub ok: bool,
    pub ended: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct AskReopenResponse {
    pub ok: bool,
    pub feedback: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct AskHistoryResponse {
    pub history: Vec<AskHistoryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct TimelineResponse {
    pub id: String,
    pub events: Vec<TimelineEvent>,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ItemSessionsResponse {
    pub sessions: Vec<SessionSummary>,
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ArtifactsResponse {
    pub artifacts: Vec<TaskArtifact>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FeedResponse {
    pub id: String,
    pub feed: Vec<FeedItem>,
    pub count: usize,
}

// Inner variants of `AdvisorResponse` below. `#[serde(deny_unknown_fields)]`
// is intentionally omitted: serde's internally-tagged enum deserialization
// passes the `kind` discriminator into the inner deserializer, so a strict
// inner struct would reject it as an unknown field. Strictness is enforced
// at the enum level (no extra keys can appear outside the declared variants).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct AdvisorAskResponse {
    pub id: i64,
    pub ask_id: String,
    pub message: String,
    pub answer: String,
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct AdvisorActionResponse {
    pub ok: bool,
    pub intent: String,
    pub feedback: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AdvisorResponse {
    Ask(AdvisorAskResponse),
    Action(AdvisorActionResponse),
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct PrSummaryResponse {
    pub pr_number: Option<i64>,
    pub summary: Option<String>,
    pub summary_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ScoutResponse {
    pub items: Vec<ScoutItem>,
    pub count: usize,
    pub total: usize,
    pub page: usize,
    pub pages: usize,
    pub per_page: usize,
    pub filter: Option<String>,
    pub status_counts: Option<HashMap<String, usize>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ScoutAddResponse {
    pub added: bool,
    pub id: i64,
    pub url: String,
    #[serde(rename = "type")]
    pub item_type: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ScoutArticleResponse {
    pub id: i64,
    pub title: Option<String>,
    pub article: Option<String>,
    #[serde(rename = "telegraphUrl")]
    pub telegraph_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ScoutDeleteResponse {
    pub removed: bool,
    pub id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ProcessResponse {
    pub ok: bool,
    pub processed: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ActResponse {
    pub ok: Option<bool>,
    pub task_id: Option<String>,
    pub title: Option<String>,
    pub skipped: Option<bool>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct MergeResponse {
    pub status: String,
    pub item_id: i64,
    pub pr: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct WorkbenchesResponse {
    pub workbenches: Vec<WorkbenchItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct CreateWorktreeResponse {
    pub ok: bool,
    pub path: String,
    pub branch: String,
    pub project: String,
    #[serde(rename = "workbenchId")]
    pub workbench_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ProjectAddResponse {
    pub ok: bool,
    pub name: String,
    pub path: String,
    #[serde(rename = "githubRepo")]
    pub github_repo: Option<String>,
    pub logo: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ProjectsListResponse {
    pub projects: Vec<ProjectSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ProjectUpsertResponse {
    pub ok: bool,
    pub name: String,
    pub path: String,
    #[serde(rename = "githubRepo")]
    pub github_repo: Option<String>,
    pub logo: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ProjectPatchResponse {
    pub ok: bool,
    pub logo: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ProjectDeleteResponse {
    pub ok: bool,
    pub deleted_tasks: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ConfigStatusResponse {
    pub exists: bool,
    #[serde(rename = "setupComplete")]
    pub setup_complete: bool,
    pub error: Option<String>,
    #[serde(rename = "taskDbPath")]
    pub task_db_path: String,
    #[serde(rename = "workerHealthPath")]
    pub worker_health_path: String,
    #[serde(rename = "lockfilePath")]
    pub lockfile_path: String,
    #[serde(rename = "configuredTaskDbPath")]
    pub configured_task_db_path: String,
    #[serde(rename = "configuredWorkerHealthPath")]
    pub configured_worker_health_path: String,
    #[serde(rename = "configuredLockfilePath")]
    pub configured_lockfile_path: String,
    #[serde(rename = "restartRequired")]
    pub restart_required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ConfigSetupResponse {
    pub ok: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ConfigPathsResponse {
    #[serde(rename = "dataDir")]
    pub data_dir: String,
    #[serde(rename = "configPath")]
    pub config_path: String,
    #[serde(rename = "taskDbPath")]
    pub task_db_path: String,
    #[serde(rename = "workerHealthPath")]
    pub worker_health_path: String,
    #[serde(rename = "lockfilePath")]
    pub lockfile_path: String,
    #[serde(rename = "configuredTaskDbPath")]
    pub configured_task_db_path: String,
    #[serde(rename = "configuredWorkerHealthPath")]
    pub configured_worker_health_path: String,
    #[serde(rename = "configuredLockfilePath")]
    pub configured_lockfile_path: String,
    #[serde(rename = "restartRequired")]
    pub restart_required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ConfigSaveResponse {
    pub ok: bool,
    #[serde(rename = "restartRequired")]
    pub restart_required: bool,
    #[serde(rename = "taskDbPath")]
    pub task_db_path: String,
    #[serde(rename = "workerHealthPath")]
    pub worker_health_path: String,
    #[serde(rename = "lockfilePath")]
    pub lockfile_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ConfigWriteResponse {
    pub ok: bool,
    #[serde(rename = "restartRequired")]
    pub restart_required: bool,
    #[serde(rename = "taskDbPath")]
    pub task_db_path: String,
    #[serde(rename = "workerHealthPath")]
    pub worker_health_path: String,
    #[serde(rename = "lockfilePath")]
    pub lockfile_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ResearchStartResponse {
    pub run_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct TelegraphPublishResponse {
    pub ok: bool,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct BoolTouchedResponse {
    pub ok: bool,
    pub touched: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct WorktreeListItem {
    pub project: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct WorktreeListResponse {
    pub worktrees: Vec<WorktreeListItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ClientLogBatchResponse {
    pub accepted: usize,
}
