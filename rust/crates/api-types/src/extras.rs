use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{ArtifactMedia, TelegramHealth, TelegramMode};

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ChannelStatus {
    pub name: String,
    pub enabled: bool,
    pub running: bool,
    pub mode: TelegramMode,
    pub token: String,
    pub owner: String,
    #[serde(rename = "lastError")]
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ChannelsResponse {
    pub channels: Vec<ChannelStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct TelegramOwnerRequest {
    pub owner: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct NotifyRequest {
    pub message: String,
    pub chat_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct NotifyResponse {
    pub ok: bool,
    pub chat_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FirecrawlScrapeRequest {
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct FirecrawlScrapeResponse {
    pub ok: bool,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct WorktreePruneError {
    pub project: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct WorktreePruneResponse {
    pub ok: bool,
    pub pruned: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorktreeCleanupRequest {
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct WorktreeCleanupResponse {
    pub ok: bool,
    pub orphans: Vec<String>,
    pub removed: Vec<String>,
    pub prune_errors: Vec<WorktreePruneError>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum UiDesiredState {
    Running,
    Suppressed,
    Updating,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UiHealthResponse {
    pub desired_state: UiDesiredState,
    #[ts(type = "number | null")]
    pub current_pid: Option<i32>,
    pub launch_available: bool,
    pub running: bool,
    pub last_error: Option<String>,
    pub degraded: bool,
    #[ts(type = "number")]
    pub restart_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SystemHealthResponse {
    pub healthy: bool,
    pub version: String,
    #[ts(type = "number")]
    pub pid: u32,
    #[ts(type = "number")]
    pub uptime: u64,
    #[ts(type = "number")]
    pub active_workers: usize,
    #[ts(type = "number")]
    pub total_items: usize,
    pub captain_degraded: bool,
    pub projects: Vec<String>,
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
    pub telegram: TelegramHealth,
    pub ui: UiHealthResponse,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct TriageItemResponse {
    pub task_id: String,
    pub pr_number: i64,
    pub project: String,
    pub github_repo: String,
    pub title: String,
    pub fast_track: bool,
    pub cursor_risk: Option<String>,
    #[ts(type = "number")]
    pub file_count: usize,
    pub fetch_failed: bool,
    pub fetch_error: String,
    #[ts(type = "number")]
    pub merge_readiness_score: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct TriageResponse {
    pub items: Vec<TriageItemResponse>,
    pub table: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct EvidenceFileRequest {
    pub filename: String,
    pub ext: String,
    pub caption: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct TaskEvidenceRequest {
    pub files: Vec<EvidenceFileRequest>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct TaskEvidenceResponse {
    pub artifact_id: i64,
    pub task_id: i64,
    pub media: Vec<ArtifactMedia>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct TaskSummaryRequest {
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct TaskSummaryResponse {
    pub artifact_id: i64,
    pub task_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ArtifactRemoteUrlPatch {
    #[ts(type = "number")]
    pub index: u32,
    pub remote_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ArtifactMediaUpdateRequest {
    pub media: Vec<ArtifactRemoteUrlPatch>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct TaskBulkUpdateRequest {
    pub ids: Vec<i64>,
    pub updates: crate::TaskBulkUpdates,
}
