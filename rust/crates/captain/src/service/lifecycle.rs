use anyhow::{bail, Result};

use crate::types::{ItemStatus, ALL_STATUSES, REWORKABLE};

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
            Self::StartMerge => "start_merge",
            Self::RetryReview => "retry_review",
        }
    }
}

pub fn infer_transition_command(
    from: ItemStatus,
    to: ItemStatus,
    planning: bool,
) -> Result<&'static str> {
    let command = match (from, to) {
        (ItemStatus::New, ItemStatus::Clarifying) => "start_clarifier",
        (ItemStatus::New, ItemStatus::Queued) => "queue",
        (ItemStatus::New, ItemStatus::CaptainReviewing) => "captain_review",
        (ItemStatus::New, ItemStatus::Canceled) => "cancel",

        (ItemStatus::Clarifying, ItemStatus::New) => "retry_clarifier",
        (ItemStatus::Clarifying, ItemStatus::NeedsClarification) => "needs_clarification",
        (ItemStatus::Clarifying, ItemStatus::Queued) => "clarifier_ready",
        (ItemStatus::Clarifying, ItemStatus::CompletedNoPr) => "clarifier_answered",
        (ItemStatus::Clarifying, ItemStatus::CaptainReviewing) => "clarifier_escalated",
        (ItemStatus::Clarifying, ItemStatus::Canceled) => "cancel",

        (ItemStatus::NeedsClarification, ItemStatus::Clarifying) => "resume_clarifier",
        (ItemStatus::NeedsClarification, ItemStatus::CaptainReviewing) => "captain_review",
        (ItemStatus::NeedsClarification, ItemStatus::HandedOff) => "handoff",
        (ItemStatus::NeedsClarification, ItemStatus::Canceled) => "cancel",

        (ItemStatus::PlanReady, ItemStatus::Queued) => "queue",
        (ItemStatus::PlanReady, ItemStatus::CaptainReviewing) => "captain_review",
        (ItemStatus::PlanReady, ItemStatus::HandedOff) => "handoff",
        (ItemStatus::PlanReady, ItemStatus::Canceled) => "cancel",

        (ItemStatus::Queued, ItemStatus::InProgress) => "spawn_worker",
        (ItemStatus::Queued, ItemStatus::HandedOff) => "handoff",
        (ItemStatus::Queued, ItemStatus::CaptainReviewing) => "captain_review",
        (ItemStatus::Queued, ItemStatus::Canceled) => "cancel",

        (ItemStatus::InProgress, ItemStatus::AwaitingReview) => "await_review",
        (ItemStatus::InProgress, ItemStatus::CompletedNoPr) => "complete_no_pr",
        (ItemStatus::InProgress, ItemStatus::PlanReady) => {
            if planning {
                "planning_ready"
            } else {
                bail!(
                    "illegal task transition {} -> {}",
                    from.as_str(),
                    to.as_str()
                )
            }
        }
        (ItemStatus::InProgress, ItemStatus::HandedOff) => "handoff",
        (ItemStatus::InProgress, ItemStatus::CaptainReviewing) => {
            if planning {
                "planning_review"
            } else {
                "captain_review"
            }
        }
        (ItemStatus::InProgress, ItemStatus::Queued) => {
            if planning {
                "recover_orphaned_planning"
            } else {
                "requeue"
            }
        }
        (ItemStatus::InProgress, ItemStatus::Errored) => {
            if planning {
                "planning_failed"
            } else {
                "worker_failed"
            }
        }
        (ItemStatus::InProgress, ItemStatus::Canceled) => "cancel",

        (ItemStatus::AwaitingReview, ItemStatus::CaptainMerging) => "start_merge",
        (ItemStatus::AwaitingReview, ItemStatus::Rework) => "rework",
        (ItemStatus::AwaitingReview, ItemStatus::Merged) => "accept",
        (ItemStatus::AwaitingReview, ItemStatus::InProgress) => "resume_worker",
        (ItemStatus::AwaitingReview, ItemStatus::Queued) => "reopen_queued",
        (ItemStatus::AwaitingReview, ItemStatus::HandedOff) => "handoff",
        (ItemStatus::AwaitingReview, ItemStatus::CaptainReviewing) => "captain_review",
        (ItemStatus::AwaitingReview, ItemStatus::Canceled) => "cancel",

        (ItemStatus::Rework, ItemStatus::Queued) => "queue",
        (ItemStatus::Rework, ItemStatus::CaptainReviewing) => "captain_review",
        (ItemStatus::Rework, ItemStatus::HandedOff) => "handoff",
        (ItemStatus::Rework, ItemStatus::Canceled) => "cancel",

        (ItemStatus::HandedOff, ItemStatus::CaptainMerging) => "start_merge",
        (ItemStatus::HandedOff, ItemStatus::Merged) => "accept",
        (ItemStatus::HandedOff, ItemStatus::Rework) => "rework",
        (ItemStatus::HandedOff, ItemStatus::InProgress) => "resume_worker",
        (ItemStatus::HandedOff, ItemStatus::Queued) => "reopen_queued",
        (ItemStatus::HandedOff, ItemStatus::CaptainReviewing) => "captain_review",
        (ItemStatus::HandedOff, ItemStatus::Canceled) => "cancel",

        (ItemStatus::Escalated, ItemStatus::Merged) => "accept",
        (ItemStatus::Escalated, ItemStatus::Rework) => "rework",
        (ItemStatus::Escalated, ItemStatus::InProgress) => "resume_worker",
        (ItemStatus::Escalated, ItemStatus::Queued) => "reopen_queued",
        (ItemStatus::Escalated, ItemStatus::HandedOff) => "handoff",
        (ItemStatus::Escalated, ItemStatus::CaptainReviewing) => "captain_review",
        (ItemStatus::Escalated, ItemStatus::Canceled) => "cancel",

        (ItemStatus::Errored, ItemStatus::CaptainReviewing) => "retry_review",
        (ItemStatus::Errored, ItemStatus::Rework) => "rework",
        (ItemStatus::Errored, ItemStatus::InProgress) => "resume_worker",
        (ItemStatus::Errored, ItemStatus::Queued) => "reopen_queued",
        (ItemStatus::Errored, ItemStatus::HandedOff) => "handoff",
        (ItemStatus::Errored, ItemStatus::Merged) => "accept",
        (ItemStatus::Errored, ItemStatus::Canceled) => "cancel",

        (ItemStatus::CaptainReviewing, ItemStatus::AwaitingReview) => "captain_ship",
        (ItemStatus::CaptainReviewing, ItemStatus::CompletedNoPr) => "captain_ship",
        (ItemStatus::CaptainReviewing, ItemStatus::CaptainReviewing) => "captain_review",
        (ItemStatus::CaptainReviewing, ItemStatus::InProgress) => "captain_resume",
        (ItemStatus::CaptainReviewing, ItemStatus::Queued) => "captain_respawn",
        (ItemStatus::CaptainReviewing, ItemStatus::Escalated) => "captain_escalate",
        (ItemStatus::CaptainReviewing, ItemStatus::New) => "retry_clarifier",
        (ItemStatus::CaptainReviewing, ItemStatus::Errored) => "captain_review_failed",
        (ItemStatus::CaptainReviewing, ItemStatus::Canceled) => "cancel",

        (ItemStatus::CaptainMerging, ItemStatus::Merged) => "merge_complete",
        (ItemStatus::CaptainMerging, ItemStatus::CaptainMerging) => "merge_spawn",
        (ItemStatus::CaptainMerging, ItemStatus::CaptainReviewing) => "merge_failed_review",
        (ItemStatus::CaptainMerging, ItemStatus::Errored) => "merge_failed",
        (ItemStatus::CaptainMerging, ItemStatus::Canceled) => "cancel",

        _ => {
            bail!(
                "illegal task transition {} -> {}",
                from.as_str(),
                to.as_str()
            )
        }
    };
    Ok(command)
}

/// Every `from` state that legally transitions into `to` (for non-planning
/// tasks). The result is derived from `infer_transition_command` so the
/// transition table stays the single source of truth — a new edge added
/// there is automatically picked up here.
///
/// Callers that want the planning-only edges use `valid_predecessors_for`
/// with `planning = true`.
pub fn valid_predecessors(to: ItemStatus) -> Vec<ItemStatus> {
    valid_predecessors_for(to, false)
}

pub fn valid_predecessors_for(to: ItemStatus, planning: bool) -> Vec<ItemStatus> {
    ALL_STATUSES
        .iter()
        .copied()
        .filter(|from| infer_transition_command(*from, to, planning).is_ok())
        .collect()
}

pub fn decide_transition(
    from: ItemStatus,
    planning: bool,
    to: ItemStatus,
) -> Result<TaskTransitionDecision> {
    let command = infer_transition_command(from, to, planning)?;
    Ok(TaskTransitionDecision { from, to, command })
}

pub fn apply_transition(task: &mut crate::Task, to: ItemStatus) -> Result<TaskTransitionDecision> {
    let decision = decide_transition(task.status, task.planning, to)?;
    task.status = decision.to;
    Ok(decision)
}

pub fn restore_status(task: &mut crate::Task, status: ItemStatus) {
    task.status = status;
}

pub fn apply_manual_transition(
    current: ItemStatus,
    command: TaskLifecycleCommand,
) -> Result<ItemStatus> {
    let invalid = || -> anyhow::Error {
        crate::TaskActionError::InvalidTransition {
            command: command.as_str(),
            status: current.as_str(),
        }
        .into()
    };
    let next = match command {
        TaskLifecycleCommand::Queue => match current {
            ItemStatus::New | ItemStatus::PlanReady | ItemStatus::Rework => ItemStatus::Queued,
            _ => return Err(invalid()),
        },
        TaskLifecycleCommand::Accept => match current {
            ItemStatus::AwaitingReview | ItemStatus::HandedOff | ItemStatus::Escalated => {
                ItemStatus::Merged
            }
            _ => return Err(invalid()),
        },
        TaskLifecycleCommand::Cancel => match current {
            status if status.is_finalized() => {
                return Err(crate::TaskActionError::FinalizedState(status.as_str()).into());
            }
            _ => ItemStatus::Canceled,
        },
        TaskLifecycleCommand::Rework => {
            if REWORKABLE.contains(&current) {
                ItemStatus::Rework
            } else {
                return Err(invalid());
            }
        }
        TaskLifecycleCommand::Handoff => match current {
            ItemStatus::InProgress
            | ItemStatus::Queued
            | ItemStatus::AwaitingReview
            | ItemStatus::NeedsClarification
            | ItemStatus::Escalated
            | ItemStatus::Errored
            | ItemStatus::Rework
            | ItemStatus::PlanReady => ItemStatus::HandedOff,
            _ => return Err(invalid()),
        },
        TaskLifecycleCommand::StartMerge => match current {
            ItemStatus::AwaitingReview | ItemStatus::HandedOff => ItemStatus::CaptainMerging,
            _ => return Err(invalid()),
        },
        TaskLifecycleCommand::RetryReview => match current {
            ItemStatus::Errored => ItemStatus::CaptainReviewing,
            _ => return Err(invalid()),
        },
    };
    Ok(next)
}

pub fn apply_manual_command(
    task: &mut crate::Task,
    command: TaskLifecycleCommand,
) -> Result<TaskTransitionDecision> {
    let from = task.status;
    let to = apply_manual_transition(from, command)?;
    task.status = to;
    Ok(TaskTransitionDecision {
        from,
        to,
        command: command.as_str(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infer_transition_command_allows_planning_ready() {
        assert_eq!(
            infer_transition_command(ItemStatus::InProgress, ItemStatus::PlanReady, true).unwrap(),
            "planning_ready"
        );
        assert!(
            infer_transition_command(ItemStatus::InProgress, ItemStatus::PlanReady, false).is_err()
        );
    }

    #[test]
    fn infer_transition_command_allows_handoff_from_awaiting_review() {
        assert_eq!(
            infer_transition_command(ItemStatus::AwaitingReview, ItemStatus::HandedOff, false)
                .unwrap(),
            "handoff"
        );
    }

    #[test]
    fn infer_transition_command_allows_cancel_from_transient_states() {
        for from in [
            ItemStatus::Clarifying,
            ItemStatus::CaptainReviewing,
            ItemStatus::CaptainMerging,
        ] {
            assert_eq!(
                infer_transition_command(from, ItemStatus::Canceled, false).unwrap(),
                "cancel"
            );
        }
    }

    #[test]
    fn valid_predecessors_matches_transition_table() {
        // NeedsClarification -> Clarifying is the edge we depend on in the
        // clarify-route fix; if the transition table ever loses it, this
        // test pins the regression.
        let preds = valid_predecessors(ItemStatus::Clarifying);
        assert!(
            preds.contains(&ItemStatus::NeedsClarification),
            "needs-clarification must be a valid predecessor of clarifying"
        );
        assert!(
            preds.contains(&ItemStatus::New),
            "new must be a valid predecessor of clarifying"
        );

        // Queued has several legal predecessors including Clarifying and
        // PlanReady; spot check a handful so the full table stays wired up.
        let queued_preds = valid_predecessors(ItemStatus::Queued);
        for expected in [
            ItemStatus::New,
            ItemStatus::Clarifying,
            ItemStatus::PlanReady,
            ItemStatus::Rework,
        ] {
            assert!(
                queued_preds.contains(&expected),
                "{expected:?} must be a valid predecessor of queued"
            );
        }

        // Every predecessor the table returns must itself be a legal edge.
        for &to in ALL_STATUSES.iter() {
            for from in valid_predecessors(to) {
                assert!(
                    infer_transition_command(from, to, false).is_ok(),
                    "{from:?} -> {to:?} should be legal"
                );
            }
        }
    }

    #[test]
    fn infer_transition_command_allows_merge_spawn_self_transition() {
        assert_eq!(
            infer_transition_command(
                ItemStatus::CaptainMerging,
                ItemStatus::CaptainMerging,
                false
            )
            .unwrap(),
            "merge_spawn"
        );
    }
}
