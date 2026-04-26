use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::SessionStatus;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum ItemStatus {
    #[serde(rename = "new")]
    New,
    #[serde(rename = "clarifying")]
    Clarifying,
    #[serde(rename = "needs-clarification")]
    NeedsClarification,
    #[serde(rename = "queued")]
    Queued,
    #[serde(rename = "in-progress")]
    InProgress,
    #[serde(rename = "captain-reviewing")]
    CaptainReviewing,
    #[serde(rename = "captain-merging")]
    CaptainMerging,
    #[serde(rename = "awaiting-review")]
    AwaitingReview,
    #[serde(rename = "rework")]
    Rework,
    #[serde(rename = "handed-off")]
    HandedOff,
    #[serde(rename = "escalated")]
    Escalated,
    #[serde(rename = "errored")]
    Errored,
    #[serde(rename = "merged")]
    Merged,
    #[serde(rename = "completed-no-pr")]
    CompletedNoPr,
    #[serde(rename = "plan-ready")]
    PlanReady,
    #[serde(rename = "canceled")]
    Canceled,
    #[serde(rename = "stopped")]
    Stopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum ScoutItemStatus {
    #[serde(rename = "pending")]
    Pending,
    #[serde(rename = "fetched")]
    Fetched,
    #[serde(rename = "processed")]
    Processed,
    #[serde(rename = "saved")]
    Saved,
    #[serde(rename = "archived")]
    Archived,
    #[serde(rename = "error")]
    Error,
}

impl ScoutItemStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Fetched => "fetched",
            Self::Processed => "processed",
            Self::Saved => "saved",
            Self::Archived => "archived",
            Self::Error => "error",
        }
    }
}

impl std::fmt::Display for ScoutItemStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str((*self).as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
pub enum ScoutResearchRunStatus {
    #[serde(rename = "running")]
    Running,
    #[serde(rename = "done")]
    Done,
    #[serde(rename = "failed")]
    Failed,
}

impl ScoutResearchRunStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Done => "done",
            Self::Failed => "failed",
        }
    }
}

impl std::fmt::Display for ScoutResearchRunStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str((*self).as_str())
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ReviewTrigger {
    GatesPass,
    Timeout,
    BrokenSession,
    BudgetExhausted,
    ClarifierFail,
    SpawnFail,
    RebaseFail,
    CiFailure,
    DegradedContext,
    Retry,
    CaptainDecision,
    MergeFail,
    RepeatedNudge,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SessionIds {
    pub worker: Option<String>,
    pub review: Option<String>,
    pub clarifier: Option<String>,
    pub merge: Option<String>,
    pub ask: Option<String>,
    pub advisor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct TaskItem {
    pub id: i64,
    pub rev: i64,
    pub title: String,
    pub status: ItemStatus,
    pub project: Option<String>,
    pub github_repo: Option<String>,
    pub branch: Option<String>,
    pub pr_number: Option<i64>,
    pub project_id: Option<i64>,
    pub worker: Option<String>,
    pub session_ids: Option<SessionIds>,
    pub intervention_count: i64,
    pub captain_review_trigger: Option<ReviewTrigger>,
    pub escalation_report: Option<String>,
    pub context: Option<String>,
    pub original_prompt: Option<String>,
    pub workbench_id: i64,
    pub worktree: Option<String>,
    pub plan: Option<String>,
    pub no_pr: bool,
    pub no_auto_merge: bool,
    pub planning: bool,
    /// Set by the clarifier when it identifies the task as fixing existing
    /// broken behavior (vs. a new feature, refactor, or research). Worker
    /// prompts use this to require reproduce-first + before-state evidence;
    /// captain review requires both before+after evidence to ship.
    pub is_bug_fix: bool,
    pub resource: Option<String>,
    pub images: Option<String>,
    pub created_at: Option<String>,
    pub last_activity_at: Option<String>,
    pub worker_started_at: Option<String>,
    pub worker_seq: i64,
    pub reopen_seq: i64,
    pub reopened_at: Option<String>,
    pub reopen_source: Option<String>,
    pub review_fail_count: i64,
    pub clarifier_fail_count: i64,
    pub spawn_fail_count: i64,
    pub merge_fail_count: i64,
    pub source: Option<String>,
    /// Unix seconds after which captain may dispatch this task again.
    /// Set when every healthy credential is in rate-limit cooldown and
    /// the failover layer surfaces `AllCredentialsExhausted`. Captain
    /// tick excludes tasks where `paused_until > unixepoch()`; UI shows
    /// "Paused until HH:MM". `None` means not paused.
    pub paused_until: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct DailyMerge {
    pub date: String,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct WorkerDetail {
    pub id: i64,
    pub title: String,
    pub status: Option<ItemStatus>,
    pub project: String,
    pub github_repo: Option<String>,
    pub branch: Option<String>,
    pub cc_session_id: Option<String>,
    pub worker: Option<String>,
    pub worktree: Option<String>,
    pub pr_number: Option<i64>,
    pub started_at: Option<String>,
    pub last_activity_at: Option<String>,
    pub intervention_count: Option<i64>,
    pub nudge_count: Option<u32>,
    pub nudge_budget: Option<u32>,
    pub last_action: Option<String>,
    pub pid: Option<u32>,
    pub is_stale: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ScoutItem {
    pub id: i64,
    pub rev: i64,
    pub url: String,
    pub title: Option<String>,
    pub status: ScoutItemStatus,
    pub item_type: Option<String>,
    pub summary: Option<String>,
    pub has_summary: Option<bool>,
    pub relevance: Option<i64>,
    pub quality: Option<i64>,
    pub date_added: Option<String>,
    pub date_processed: Option<String>,
    pub added_by: Option<String>,
    pub source_name: Option<String>,
    pub date_published: Option<String>,
    pub error_count: Option<i64>,
    pub research_run_id: Option<i64>,
    #[serde(rename = "telegraphUrl")]
    pub telegraph_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ScoutResearchRun {
    pub id: i64,
    pub research_prompt: String,
    pub status: ScoutResearchRunStatus,
    pub error: Option<String>,
    pub session_id: Option<String>,
    pub added_count: i64,
    pub created_at: String,
    pub completed_at: Option<String>,
    pub rev: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ClarifierQuestion {
    pub question: String,
    pub answer: Option<String>,
    pub self_answered: bool,
    pub category: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct TimelineEvent {
    pub timestamp: String,
    pub actor: String,
    pub summary: String,
    /// Tagged by `event_type`. See [`crate::TimelineEventPayload`] variants
    /// for the exact shape per event kind.
    pub data: crate::TimelineEventPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct AskHistoryEntry {
    pub ask_id: String,
    pub session_id: String,
    pub role: String,
    pub content: String,
    pub timestamp: String,
    pub intent: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS)]
pub enum ArtifactType {
    #[serde(rename = "evidence")]
    Evidence,
    #[serde(rename = "work_summary")]
    WorkSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ArtifactMedia {
    pub index: u32,
    pub filename: String,
    pub ext: String,
    pub local_path: Option<String>,
    pub remote_url: Option<String>,
    pub caption: Option<String>,
    pub kind: Option<crate::extras::EvidenceKind>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct TaskArtifact {
    pub id: i64,
    pub task_id: i64,
    pub artifact_type: ArtifactType,
    pub content: String,
    pub media: Vec<ArtifactMedia>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
// TimelineEvent carries the big TimelineEventPayload union; keeping it inline
// on the Timeline variant makes the enum ~400 bytes vs. ~40 for Artifact /
// Message. Expected: the timeline is the dominant feed entry, so inlining is
// the right tradeoff.
#[allow(clippy::large_enum_variant)]
pub enum FeedItem {
    Timeline {
        timestamp: String,
        data: TimelineEvent,
    },
    Artifact {
        timestamp: String,
        data: TaskArtifact,
    },
    Message {
        timestamp: String,
        data: AskHistoryEntry,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WorkbenchItem {
    pub id: i64,
    pub rev: i64,
    pub project_id: i64,
    pub project: String,
    pub worktree: String,
    pub title: String,
    pub created_at: String,
    pub last_activity_at: String,
    pub pinned_at: Option<String>,
    pub archived_at: Option<String>,
    pub deleted_at: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum TerminalAgent {
    #[default]
    Claude,
    Codex,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum TerminalState {
    Live,
    Restored,
    Exited,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum TelegramMode {
    Embedded,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct TerminalSessionInfo {
    pub id: String,
    pub rev: u64,
    pub project: String,
    pub cwd: String,
    pub agent: TerminalAgent,
    pub running: bool,
    pub exit_code: Option<u32>,
    pub state: Option<TerminalState>,
    pub restored: Option<bool>,
    #[serde(rename = "createdAt")]
    pub created_at: Option<String>,
    #[serde(rename = "endedAt")]
    pub ended_at: Option<String>,
    #[serde(rename = "terminalId")]
    pub terminal_id: Option<String>,
    pub name: Option<String>,
    #[serde(rename = "ccSessionId")]
    pub cc_session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TelegramHealth {
    pub enabled: bool,
    pub running: bool,
    pub owner: String,
    pub last_error: Option<String>,
    pub degraded: bool,
    pub restart_count: u64,
    pub mode: TelegramMode,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ScoutItemSession {
    pub session_id: String,
    pub caller: String,
    pub status: SessionStatus,
    pub created_at: String,
    pub model: Option<String>,
    pub duration_ms: Option<i64>,
    pub cost_usd: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ProjectSummary {
    pub key: String,
    pub name: String,
    pub path: String,
    pub github_repo: Option<String>,
    pub logo: Option<String>,
    pub aliases: Vec<String>,
    pub hooks: std::collections::HashMap<String, String>,
    pub worker_preamble: String,
    pub scout_summary: String,
    pub check_command: String,
}
