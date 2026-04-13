//! Timeline event types for task history.

use serde::{Deserialize, Serialize};

/// Type of a timeline event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TimelineEventType {
    #[serde(rename = "created")]
    Created,
    #[serde(rename = "clarify_started")]
    ClarifyStarted,
    #[serde(rename = "clarify_question")]
    ClarifyQuestion,
    #[serde(rename = "clarify_resolved")]
    ClarifyResolved,
    #[serde(rename = "human_answered")]
    HumanAnswered,
    #[serde(rename = "worker_spawned")]
    WorkerSpawned,
    #[serde(rename = "worker_nudged")]
    WorkerNudged,
    #[serde(rename = "session_resumed")]
    SessionResumed,
    #[serde(rename = "worker_completed")]
    WorkerCompleted,
    #[serde(rename = "captain_review_started")]
    CaptainReviewStarted,
    #[serde(rename = "captain_review_verdict")]
    CaptainReviewVerdict,
    #[serde(rename = "captain_merge_started")]
    CaptainMergeStarted,
    #[serde(rename = "awaiting_review")]
    AwaitingReview,
    #[serde(rename = "human_reopen")]
    HumanReopen,
    #[serde(rename = "human_ask")]
    HumanAsk,
    #[serde(rename = "rebase_triggered")]
    RebaseTriggered,
    #[serde(rename = "rework_requested")]
    ReworkRequested,
    #[serde(rename = "merged")]
    Merged,
    #[serde(rename = "escalated")]
    Escalated,
    #[serde(rename = "errored")]
    Errored,
    #[serde(rename = "canceled")]
    Canceled,
    #[serde(rename = "handed_off")]
    HandedOff,
    #[serde(rename = "completed_no_pr")]
    CompletedNoPr,
    #[serde(rename = "status_changed")]
    StatusChanged,
    #[serde(rename = "rate_limited")]
    RateLimited,
    #[serde(rename = "worker_reopened")]
    WorkerReopened,
    #[serde(rename = "human_ask_failed")]
    HumanAskFailed,
    #[serde(rename = "evidence_updated")]
    EvidenceUpdated,
    #[serde(rename = "work_summary_updated")]
    WorkSummaryUpdated,
}

/// A single event in a task's timeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineEvent {
    pub event_type: TimelineEventType,
    pub timestamp: String,
    pub actor: String,
    pub summary: String,
    #[serde(default)]
    pub data: serde_json::Value,
}
