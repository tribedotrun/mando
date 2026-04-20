use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{MandoConfig, ScoutItem, TaskItem, TerminalSessionInfo, WorkbenchItem, WorkerDetail};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
pub enum NotifyLevel {
    Low,
    Normal,
    High,
    Critical,
}

impl NotifyLevel {
    pub const fn value(self) -> u8 {
        match self {
            NotifyLevel::Low => 10,
            NotifyLevel::Normal => 20,
            NotifyLevel::High => 30,
            NotifyLevel::Critical => 40,
        }
    }
}

#[cfg(test)]
mod notify_level_order_tests {
    use super::NotifyLevel;

    #[test]
    fn notify_level_total_order_is_low_normal_high_critical() {
        assert!(NotifyLevel::Low < NotifyLevel::Normal);
        assert!(NotifyLevel::Normal < NotifyLevel::High);
        assert!(NotifyLevel::High < NotifyLevel::Critical);
        let mut sorted = vec![
            NotifyLevel::Critical,
            NotifyLevel::Low,
            NotifyLevel::High,
            NotifyLevel::Normal,
        ];
        sorted.sort();
        assert_eq!(
            sorted,
            vec![
                NotifyLevel::Low,
                NotifyLevel::Normal,
                NotifyLevel::High,
                NotifyLevel::Critical,
            ]
        );
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "type")]
pub enum NotificationKind {
    Escalated {
        item_id: String,
        summary: Option<String>,
    },
    NeedsClarification {
        item_id: String,
        questions: Option<String>,
    },
    RateLimited {
        status: String,
        utilization: Option<f64>,
        resets_at: Option<u64>,
        rate_limit_type: Option<String>,
        overage_status: Option<String>,
        overage_resets_at: Option<u64>,
        overage_disabled_reason: Option<String>,
    },
    ScoutProcessed {
        scout_id: i64,
        title: String,
        relevance: i64,
        quality: i64,
        source_name: Option<String>,
        telegraph_url: Option<String>,
    },
    ScoutProcessFailed {
        scout_id: i64,
        url: String,
        error: String,
    },
    AdvisorAnswered {
        item_id: String,
        title: String,
    },
    Generic,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct NotificationPayload {
    pub message: String,
    pub level: NotifyLevel,
    pub kind: NotificationKind,
    pub task_key: Option<String>,
    pub reply_markup: Option<crate::TelegramReplyMarkup>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SseDaemonInfo {
    pub version: String,
    #[ts(type = "number")]
    pub uptime: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SseSnapshotData {
    pub tasks: Vec<TaskItem>,
    pub workers: Vec<WorkerDetail>,
    pub workbenches: Vec<WorkbenchItem>,
    pub terminals: Vec<TerminalSessionInfo>,
    pub config: MandoConfig,
    pub daemon: SseDaemonInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SseSnapshotErrorData {
    pub message: String,
    pub retry: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SseResyncData {
    pub reason: String,
    pub reload: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct TaskEventData {
    pub action: Option<String>,
    pub item: Option<TaskItem>,
    pub id: Option<i64>,
    pub cleared_by: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ScoutEventData {
    pub action: Option<String>,
    pub item: Option<ScoutItem>,
    pub id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct StatusEventData {
    pub action: Option<String>,
    pub affected_task_ids: Option<Vec<i64>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SessionsEventData {
    pub affected_task_ids: Option<Vec<i64>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct WorkbenchEventData {
    pub action: Option<String>,
    pub item: Option<WorkbenchItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct CredentialsEventData {
    pub action: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ResearchLink {
    pub url: String,
    pub title: String,
    #[serde(rename = "type")]
    pub link_type: String,
    pub reason: String,
    pub id: Option<i64>,
    pub added: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ResearchError {
    pub url: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ResearchEventData {
    pub action: String,
    pub run_id: i64,
    pub research_prompt: Option<String>,
    #[ts(type = "number | null")]
    pub elapsed_s: Option<u64>,
    pub links: Option<Vec<ResearchLink>>,
    pub errors: Option<Vec<ResearchError>>,
    pub added_count: Option<i64>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ArtifactEventData {
    pub action: String,
    pub task_id: i64,
    pub artifact_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SnapshotPayload {
    #[ts(type = "number")]
    pub ts: f64,
    pub data: SseSnapshotData,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SnapshotErrorPayload {
    #[ts(type = "number")]
    pub ts: f64,
    pub data: SseSnapshotErrorData,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResyncPayload {
    #[ts(type = "number")]
    pub ts: f64,
    pub data: SseResyncData,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TasksPayload {
    #[ts(type = "number")]
    pub ts: f64,
    pub data: Option<TaskEventData>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScoutPayload {
    #[ts(type = "number")]
    pub ts: f64,
    pub data: Option<ScoutEventData>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StatusPayload {
    #[ts(type = "number")]
    pub ts: f64,
    pub data: Option<StatusEventData>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionsPayload {
    #[ts(type = "number")]
    pub ts: f64,
    pub data: Option<SessionsEventData>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NotificationEventPayload {
    #[ts(type = "number")]
    pub ts: f64,
    pub data: Option<NotificationPayload>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkbenchesPayload {
    #[ts(type = "number")]
    pub ts: f64,
    pub data: Option<WorkbenchEventData>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfigPayload {
    #[ts(type = "number")]
    pub ts: f64,
    pub data: Option<MandoConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResearchPayload {
    #[ts(type = "number")]
    pub ts: f64,
    pub data: Option<ResearchEventData>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CredentialsPayload {
    #[ts(type = "number")]
    pub ts: f64,
    pub data: Option<CredentialsEventData>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArtifactsPayload {
    #[ts(type = "number")]
    pub ts: f64,
    pub data: Option<ArtifactEventData>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case", tag = "event", content = "data")]
pub enum SseEnvelope {
    Snapshot(SnapshotPayload),
    SnapshotError(SnapshotErrorPayload),
    Resync(ResyncPayload),
    Tasks(TasksPayload),
    Scout(ScoutPayload),
    Status(StatusPayload),
    Sessions(SessionsPayload),
    Notification(NotificationEventPayload),
    Workbenches(WorkbenchesPayload),
    Config(ConfigPayload),
    Research(ResearchPayload),
    Credentials(CredentialsPayload),
    Artifacts(ArtifactsPayload),
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TerminalOutputPayload {
    pub data_b64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TerminalExitPayload {
    pub code: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case", tag = "event", content = "data")]
pub enum TerminalStreamEnvelope {
    Output(TerminalOutputPayload),
    Exit(TerminalExitPayload),
}
