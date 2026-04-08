//! ItemStatus and ReviewTrigger enums — task lifecycle state machine.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Status of a task (15 states, 3 terminal).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
    #[serde(rename = "canceled")]
    Canceled,
}

/// All 15 item statuses.
pub const ALL_STATUSES: [ItemStatus; 15] = [
    ItemStatus::New,
    ItemStatus::Clarifying,
    ItemStatus::NeedsClarification,
    ItemStatus::Queued,
    ItemStatus::InProgress,
    ItemStatus::CaptainReviewing,
    ItemStatus::CaptainMerging,
    ItemStatus::AwaitingReview,
    ItemStatus::Rework,
    ItemStatus::HandedOff,
    ItemStatus::Escalated,
    ItemStatus::Errored,
    ItemStatus::Merged,
    ItemStatus::CompletedNoPr,
    ItemStatus::Canceled,
];

/// Terminal statuses — no further work expected.
pub const FINALIZED: [ItemStatus; 3] = [
    ItemStatus::Merged,
    ItemStatus::CompletedNoPr,
    ItemStatus::Canceled,
];

/// Statuses from which an item can be reworked (same worktree, new branch + new worker)
/// or reopened (resume existing session). Currently identical; separate names
/// kept for semantic clarity in call sites.
pub const ACTIONABLE_TERMINAL: [ItemStatus; 4] = [
    ItemStatus::AwaitingReview,
    ItemStatus::HandedOff,
    ItemStatus::Escalated,
    ItemStatus::Errored,
];
pub const REWORKABLE: [ItemStatus; 4] = ACTIONABLE_TERMINAL;
pub const REOPENABLE: [ItemStatus; 4] = ACTIONABLE_TERMINAL;

impl ItemStatus {
    #[must_use]
    pub fn is_finalized(self) -> bool {
        FINALIZED.contains(&self)
    }

    /// The serde string representation (kebab-case).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::New => "new",
            Self::Clarifying => "clarifying",
            Self::NeedsClarification => "needs-clarification",
            Self::Queued => "queued",
            Self::InProgress => "in-progress",
            Self::CaptainReviewing => "captain-reviewing",
            Self::CaptainMerging => "captain-merging",
            Self::AwaitingReview => "awaiting-review",
            Self::Rework => "rework",
            Self::HandedOff => "handed-off",
            Self::Escalated => "escalated",
            Self::Errored => "errored",
            Self::Merged => "merged",
            Self::CompletedNoPr => "completed-no-pr",
            Self::Canceled => "canceled",
        }
    }
}

impl fmt::Display for ItemStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ItemStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "new" => Ok(Self::New),
            "clarifying" => Ok(Self::Clarifying),
            "needs-clarification" => Ok(Self::NeedsClarification),
            "queued" => Ok(Self::Queued),
            "in-progress" => Ok(Self::InProgress),
            "captain-reviewing" => Ok(Self::CaptainReviewing),
            "captain-merging" => Ok(Self::CaptainMerging),
            "awaiting-review" => Ok(Self::AwaitingReview),
            "rework" => Ok(Self::Rework),
            "handed-off" => Ok(Self::HandedOff),
            "escalated" => Ok(Self::Escalated),
            "errored" => Ok(Self::Errored),
            "merged" => Ok(Self::Merged),
            "completed-no-pr" => Ok(Self::CompletedNoPr),
            "canceled" => Ok(Self::Canceled),
            _ => Err(format!("unknown status: {s}")),
        }
    }
}

/// Trigger context for a captain review.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewTrigger {
    GatesPass,
    Timeout,
    BrokenSession,
    BudgetExhausted,
    ClarifierFail,
    RebaseFail,
    CiFailure,
    DegradedContext,
    Retry,
    CaptainDecision,
    MergeFail,
    RepeatedNudge,
}

impl ReviewTrigger {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::GatesPass => "gates_pass",
            Self::Timeout => "timeout",
            Self::BrokenSession => "broken_session",
            Self::BudgetExhausted => "budget_exhausted",
            Self::ClarifierFail => "clarifier_fail",
            Self::RebaseFail => "rebase_fail",
            Self::CiFailure => "ci_failure",
            Self::DegradedContext => "degraded_context",
            Self::Retry => "retry",
            Self::CaptainDecision => "captain_decision",
            Self::MergeFail => "merge_fail",
            Self::RepeatedNudge => "repeated_nudge",
        }
    }
}

impl fmt::Display for ReviewTrigger {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ReviewTrigger {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "gates_pass" => Ok(Self::GatesPass),
            "timeout" => Ok(Self::Timeout),
            "broken_session" => Ok(Self::BrokenSession),
            "budget_exhausted" => Ok(Self::BudgetExhausted),
            "clarifier_fail" => Ok(Self::ClarifierFail),
            "rebase_fail" => Ok(Self::RebaseFail),
            "ci_failure" => Ok(Self::CiFailure),
            "degraded_context" => Ok(Self::DegradedContext),
            "retry" => Ok(Self::Retry),
            "captain_decision" => Ok(Self::CaptainDecision),
            "merge_fail" => Ok(Self::MergeFail),
            "repeated_nudge" => Ok(Self::RepeatedNudge),
            _ => Err(format!("unknown review trigger: {s}")),
        }
    }
}
