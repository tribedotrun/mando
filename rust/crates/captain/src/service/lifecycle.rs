use anyhow::Result;

use crate::types::{ItemStatus, ALL_STATUSES};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskTransitionDecision {
    pub from: ItemStatus,
    pub to: ItemStatus,
    pub command: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskLifecycleCommand {
    Queue,
    Accept,
    Cancel,
    Rework,
    Handoff,
    Stop,
    StartMerge,
    RetryReview,
}

impl TaskLifecycleCommand {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Queue => "queue",
            Self::Accept => "accept",
            Self::Cancel => "cancel",
            Self::Rework => "rework",
            Self::Handoff => "handoff",
            Self::Stop => "stop",
            Self::StartMerge => "start_merge",
            Self::RetryReview => "retry_review",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TransitionRow {
    from: ItemStatus,
    to: ItemStatus,
    command: &'static str,
    manual: Option<TaskLifecycleCommand>,
}

macro_rules! transition {
    (($from:expr, $to:expr) => $command:literal) => {
        TransitionRow {
            from: $from,
            to: $to,
            command: $command,
            manual: None,
        }
    };
    (($from:expr, $to:expr) => $command:literal, $manual:ident) => {
        TransitionRow {
            from: $from,
            to: $to,
            command: $command,
            manual: Some(TaskLifecycleCommand::$manual),
        }
    };
}

const TRANSITIONS: &[TransitionRow] = &[
    transition!((ItemStatus::New, ItemStatus::Clarifying) => "start_clarifier"),
    transition!((ItemStatus::New, ItemStatus::Queued) => "queue", Queue),
    transition!((ItemStatus::New, ItemStatus::CaptainReviewing) => "captain_review"),
    transition!((ItemStatus::New, ItemStatus::Canceled) => "cancel", Cancel),
    transition!((ItemStatus::Clarifying, ItemStatus::New) => "retry_clarifier"),
    transition!((ItemStatus::Clarifying, ItemStatus::NeedsClarification) => "needs_clarification"),
    transition!((ItemStatus::Clarifying, ItemStatus::Queued) => "clarifier_ready"),
    transition!((ItemStatus::Clarifying, ItemStatus::CompletedNoPr) => "clarifier_answered"),
    transition!((ItemStatus::Clarifying, ItemStatus::CaptainReviewing) => "clarifier_escalated"),
    transition!((ItemStatus::Clarifying, ItemStatus::Canceled) => "cancel", Cancel),
    transition!((ItemStatus::NeedsClarification, ItemStatus::Clarifying) => "resume_clarifier"),
    transition!((ItemStatus::NeedsClarification, ItemStatus::CaptainReviewing) => "captain_review"),
    transition!((ItemStatus::NeedsClarification, ItemStatus::HandedOff) => "handoff", Handoff),
    transition!((ItemStatus::NeedsClarification, ItemStatus::Canceled) => "cancel", Cancel),
    transition!((ItemStatus::Queued, ItemStatus::InProgress) => "spawn_worker"),
    transition!((ItemStatus::Queued, ItemStatus::HandedOff) => "handoff", Handoff),
    transition!((ItemStatus::Queued, ItemStatus::CaptainReviewing) => "captain_review"),
    transition!((ItemStatus::Queued, ItemStatus::Canceled) => "cancel", Cancel),
    transition!((ItemStatus::InProgress, ItemStatus::AwaitingReview) => "await_review"),
    transition!((ItemStatus::InProgress, ItemStatus::CompletedNoPr) => "complete_no_pr"),
    transition!((ItemStatus::InProgress, ItemStatus::HandedOff) => "handoff", Handoff),
    transition!((ItemStatus::InProgress, ItemStatus::Stopped) => "stop", Stop),
    transition!((ItemStatus::InProgress, ItemStatus::CaptainReviewing) => "captain_review"),
    transition!((ItemStatus::InProgress, ItemStatus::Queued) => "requeue"),
    transition!((ItemStatus::InProgress, ItemStatus::Errored) => "worker_failed"),
    transition!((ItemStatus::InProgress, ItemStatus::Canceled) => "cancel", Cancel),
    transition!((ItemStatus::AwaitingReview, ItemStatus::CaptainMerging) => "start_merge", StartMerge),
    transition!((ItemStatus::AwaitingReview, ItemStatus::Rework) => "rework", Rework),
    transition!((ItemStatus::AwaitingReview, ItemStatus::Merged) => "accept", Accept),
    transition!((ItemStatus::AwaitingReview, ItemStatus::InProgress) => "resume_worker"),
    transition!((ItemStatus::AwaitingReview, ItemStatus::Queued) => "reopen_queued"),
    transition!((ItemStatus::AwaitingReview, ItemStatus::HandedOff) => "handoff", Handoff),
    transition!((ItemStatus::AwaitingReview, ItemStatus::CaptainReviewing) => "captain_review"),
    transition!((ItemStatus::AwaitingReview, ItemStatus::Canceled) => "cancel", Cancel),
    transition!((ItemStatus::Rework, ItemStatus::Queued) => "queue", Queue),
    transition!((ItemStatus::Rework, ItemStatus::CaptainReviewing) => "captain_review"),
    transition!((ItemStatus::Rework, ItemStatus::HandedOff) => "handoff", Handoff),
    transition!((ItemStatus::Rework, ItemStatus::Canceled) => "cancel", Cancel),
    transition!((ItemStatus::HandedOff, ItemStatus::CaptainMerging) => "start_merge", StartMerge),
    transition!((ItemStatus::HandedOff, ItemStatus::Merged) => "accept", Accept),
    transition!((ItemStatus::HandedOff, ItemStatus::Rework) => "rework", Rework),
    transition!((ItemStatus::HandedOff, ItemStatus::InProgress) => "resume_worker"),
    transition!((ItemStatus::HandedOff, ItemStatus::Queued) => "reopen_queued"),
    transition!((ItemStatus::HandedOff, ItemStatus::CaptainReviewing) => "captain_review"),
    transition!((ItemStatus::HandedOff, ItemStatus::Canceled) => "cancel", Cancel),
    transition!((ItemStatus::Escalated, ItemStatus::Merged) => "accept", Accept),
    transition!((ItemStatus::Escalated, ItemStatus::Rework) => "rework", Rework),
    transition!((ItemStatus::Escalated, ItemStatus::InProgress) => "resume_worker"),
    transition!((ItemStatus::Escalated, ItemStatus::Queued) => "reopen_queued"),
    transition!((ItemStatus::Escalated, ItemStatus::HandedOff) => "handoff", Handoff),
    transition!((ItemStatus::Escalated, ItemStatus::CaptainReviewing) => "captain_review"),
    transition!((ItemStatus::Escalated, ItemStatus::Canceled) => "cancel", Cancel),
    transition!((ItemStatus::Errored, ItemStatus::CaptainReviewing) => "retry_review", RetryReview),
    transition!((ItemStatus::Errored, ItemStatus::Rework) => "rework", Rework),
    transition!((ItemStatus::Errored, ItemStatus::InProgress) => "resume_worker"),
    transition!((ItemStatus::Errored, ItemStatus::Queued) => "reopen_queued"),
    transition!((ItemStatus::Errored, ItemStatus::HandedOff) => "handoff", Handoff),
    transition!((ItemStatus::Errored, ItemStatus::Merged) => "accept"),
    transition!((ItemStatus::Errored, ItemStatus::Canceled) => "cancel", Cancel),
    transition!((ItemStatus::CompletedNoPr, ItemStatus::InProgress) => "resume_worker"),
    transition!((ItemStatus::CompletedNoPr, ItemStatus::Queued) => "reopen_queued"),
    transition!((ItemStatus::CompletedNoPr, ItemStatus::Rework) => "rework", Rework),
    transition!((ItemStatus::CompletedNoPr, ItemStatus::CaptainReviewing) => "captain_review"),
    transition!((ItemStatus::CompletedNoPr, ItemStatus::Canceled) => "cancel"),
    transition!((ItemStatus::CaptainReviewing, ItemStatus::AwaitingReview) => "captain_ship"),
    transition!((ItemStatus::CaptainReviewing, ItemStatus::CompletedNoPr) => "captain_ship"),
    transition!((ItemStatus::CaptainReviewing, ItemStatus::CaptainReviewing) => "captain_review"),
    transition!((ItemStatus::CaptainReviewing, ItemStatus::InProgress) => "captain_resume"),
    transition!((ItemStatus::CaptainReviewing, ItemStatus::Queued) => "captain_respawn"),
    transition!((ItemStatus::CaptainReviewing, ItemStatus::Escalated) => "captain_escalate"),
    transition!((ItemStatus::CaptainReviewing, ItemStatus::New) => "retry_clarifier"),
    transition!((ItemStatus::CaptainReviewing, ItemStatus::Errored) => "captain_review_failed"),
    transition!((ItemStatus::CaptainReviewing, ItemStatus::Canceled) => "cancel", Cancel),
    transition!((ItemStatus::Stopped, ItemStatus::InProgress) => "resume_worker"),
    transition!((ItemStatus::Stopped, ItemStatus::Queued) => "reopen_queued"),
    transition!((ItemStatus::Stopped, ItemStatus::CaptainReviewing) => "captain_review"),
    transition!((ItemStatus::Stopped, ItemStatus::Rework) => "rework", Rework),
    transition!((ItemStatus::Stopped, ItemStatus::Canceled) => "cancel", Cancel),
    transition!((ItemStatus::Canceled, ItemStatus::InProgress) => "resume_worker"),
    transition!((ItemStatus::Canceled, ItemStatus::Queued) => "reopen_queued"),
    transition!((ItemStatus::CaptainMerging, ItemStatus::Merged) => "merge_complete"),
    transition!((ItemStatus::CaptainMerging, ItemStatus::CaptainMerging) => "merge_spawn"),
    transition!((ItemStatus::CaptainMerging, ItemStatus::CaptainReviewing) => "merge_failed_review"),
    transition!((ItemStatus::CaptainMerging, ItemStatus::Errored) => "merge_failed"),
    transition!((ItemStatus::CaptainMerging, ItemStatus::Canceled) => "cancel", Cancel),
];

pub fn infer_transition_command(from: ItemStatus, to: ItemStatus) -> Result<&'static str> {
    TRANSITIONS
        .iter()
        .find(|row| row.from == from && row.to == to)
        .map(|row| row.command)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "illegal task transition {} -> {}",
                from.as_str(),
                to.as_str()
            )
        })
}

/// Every `from` state that legally transitions into `to`, in status order.
pub fn valid_predecessors(to: ItemStatus) -> Vec<ItemStatus> {
    ALL_STATUSES
        .iter()
        .copied()
        .filter(|from| {
            TRANSITIONS
                .iter()
                .any(|row| row.from == *from && row.to == to)
        })
        .collect()
}

pub fn decide_transition(from: ItemStatus, to: ItemStatus) -> Result<TaskTransitionDecision> {
    let command = infer_transition_command(from, to)?;
    Ok(TaskTransitionDecision { from, to, command })
}

pub fn apply_transition(task: &mut crate::Task, to: ItemStatus) -> Result<TaskTransitionDecision> {
    let decision = decide_transition(task.status, to)?;
    task.status = decision.to;
    Ok(decision)
}

/// `Clarifying → NeedsClarification` via the failure path — drops
/// `session_ids.clarifier` so the next re-answer spawns a fresh CC
/// session instead of resuming the one that just failed. Use this
/// instead of plain `apply_transition` when a fatal clarifier error
/// causes the rollback; the happy follow-up path (new question asked)
/// still uses `apply_transition` and preserves the session.
pub fn apply_clarifier_failure(task: &mut crate::Task) -> Result<TaskTransitionDecision> {
    let decision = decide_transition(task.status, ItemStatus::NeedsClarification)?;
    task.status = decision.to;
    task.session_ids.clarifier = None;
    Ok(decision)
}

/// `* → Escalated` — requires the caller to supply the escalation
/// report so Escalated is never reached with an empty audit trail.
/// Pass `Some(reason)` for diagnostic callers (unknown verdict,
/// unexpected state) even when no structured report is available.
pub fn apply_escalation(
    task: &mut crate::Task,
    report: Option<String>,
) -> Result<TaskTransitionDecision> {
    let decision = decide_transition(task.status, ItemStatus::Escalated)?;
    task.status = decision.to;
    task.escalation_report = report;
    Ok(decision)
}

pub fn restore_status(task: &mut crate::Task, status: ItemStatus) {
    task.status = status;
}

pub fn apply_manual_command(
    task: &mut crate::Task,
    command: TaskLifecycleCommand,
) -> Result<TaskTransitionDecision> {
    let decision = decide_manual_transition(task.status, command)?;
    task.status = decision.to;
    Ok(decision)
}

fn decide_manual_transition(
    current: ItemStatus,
    command: TaskLifecycleCommand,
) -> Result<TaskTransitionDecision> {
    if command == TaskLifecycleCommand::Cancel && current.is_finalized() {
        return Err(crate::TaskActionError::FinalizedState(current.as_str()).into());
    }
    TRANSITIONS
        .iter()
        .find(|row| row.from == current && row.manual == Some(command))
        .map(|row| TaskTransitionDecision {
            from: row.from,
            to: row.to,
            command: row.command,
        })
        .ok_or_else(|| {
            crate::TaskActionError::InvalidTransition {
                command: command.as_str(),
                status: current.as_str(),
            }
            .into()
        })
}
