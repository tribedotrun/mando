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

#[cfg(test)]
mod tests {
    use super::*;

    fn apply_manual_transition(
        current: ItemStatus,
        command: TaskLifecycleCommand,
    ) -> Result<ItemStatus> {
        decide_manual_transition(current, command).map(|decision| decision.to)
    }

    const ALL_COMMANDS: [TaskLifecycleCommand; 8] = [
        TaskLifecycleCommand::Queue,
        TaskLifecycleCommand::Accept,
        TaskLifecycleCommand::Cancel,
        TaskLifecycleCommand::Rework,
        TaskLifecycleCommand::Handoff,
        TaskLifecycleCommand::Stop,
        TaskLifecycleCommand::StartMerge,
        TaskLifecycleCommand::RetryReview,
    ];

    #[test]
    fn transition_rows_are_unique_and_complete() {
        assert_eq!(TRANSITIONS.len(), 85);
        for (index, row) in TRANSITIONS.iter().enumerate() {
            assert!(
                !TRANSITIONS[index + 1..]
                    .iter()
                    .any(|other| other.from == row.from && other.to == row.to),
                "duplicate transition row: {:?} -> {:?}",
                row.from,
                row.to
            );
            if let Some(command) = row.manual {
                assert!(
                    !TRANSITIONS[index + 1..]
                        .iter()
                        .any(|other| { other.from == row.from && other.manual == Some(command) }),
                    "duplicate manual command {command:?} from {:?}",
                    row.from
                );
            }
        }
    }

    #[test]
    fn transition_rows_have_exactly_two_non_manual_self_transitions() {
        let self_rows = TRANSITIONS
            .iter()
            .filter(|row| row.from == row.to)
            .copied()
            .collect::<Vec<_>>();
        assert_eq!(
            self_rows,
            vec![
                transition!((ItemStatus::CaptainReviewing, ItemStatus::CaptainReviewing) => "captain_review"),
                transition!((ItemStatus::CaptainMerging, ItemStatus::CaptainMerging) => "merge_spawn"),
            ]
        );
        assert!(self_rows.iter().all(|row| row.manual.is_none()));
    }

    #[test]
    fn every_manual_annotation_round_trips_through_the_public_api() {
        for row in TRANSITIONS.iter().filter(|row| row.manual.is_some()) {
            let command = row.manual.expect("filtered to manual rows");
            assert_eq!(apply_manual_transition(row.from, command).unwrap(), row.to);
            let decision = decide_manual_transition(row.from, command).unwrap();
            assert_eq!(decision.command, row.command);
        }

        for status in ALL_STATUSES {
            for command in ALL_COMMANDS {
                let annotated = TRANSITIONS
                    .iter()
                    .any(|row| row.from == status && row.manual == Some(command));
                let result = apply_manual_transition(status, command);
                if command == TaskLifecycleCommand::Cancel && status.is_finalized() {
                    let error = result.unwrap_err();
                    assert!(matches!(
                        crate::types::find_task_action_error(&error),
                        Some(crate::TaskActionError::FinalizedState(_))
                    ));
                } else {
                    assert_eq!(
                        result.is_ok(),
                        annotated,
                        "manual {command:?} parity from {status:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn internal_edges_do_not_expand_restricted_manual_commands() {
        assert_eq!(
            infer_transition_command(ItemStatus::Errored, ItemStatus::Merged).unwrap(),
            "accept"
        );
        assert!(
            apply_manual_transition(ItemStatus::Errored, TaskLifecycleCommand::Accept).is_err()
        );

        assert_eq!(
            infer_transition_command(ItemStatus::CompletedNoPr, ItemStatus::Canceled).unwrap(),
            "cancel"
        );
        let error =
            apply_manual_transition(ItemStatus::CompletedNoPr, TaskLifecycleCommand::Cancel)
                .unwrap_err();
        assert!(matches!(
            crate::types::find_task_action_error(&error),
            Some(crate::TaskActionError::FinalizedState("completed-no-pr"))
        ));
    }

    #[test]
    fn infer_transition_command_allows_handoff_from_awaiting_review() {
        assert_eq!(
            infer_transition_command(ItemStatus::AwaitingReview, ItemStatus::HandedOff).unwrap(),
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
                infer_transition_command(from, ItemStatus::Canceled).unwrap(),
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

        // Queued has several legal predecessors including Clarifying;
        // spot check a handful so the full table stays wired up.
        let queued_preds = valid_predecessors(ItemStatus::Queued);
        for expected in [ItemStatus::New, ItemStatus::Clarifying, ItemStatus::Rework] {
            assert!(
                queued_preds.contains(&expected),
                "{expected:?} must be a valid predecessor of queued"
            );
        }

        // Every predecessor the table returns must itself be a legal edge.
        for &to in ALL_STATUSES.iter() {
            for from in valid_predecessors(to) {
                assert!(
                    infer_transition_command(from, to).is_ok(),
                    "{from:?} -> {to:?} should be legal"
                );
            }
        }
    }

    #[test]
    fn apply_clarifier_failure_drops_clarifier_session() {
        let mut task = crate::Task::new("t");
        task.set_status_for_tests(ItemStatus::Clarifying);
        task.session_ids.clarifier = Some("poisoned".into());
        let decision = apply_clarifier_failure(&mut task).unwrap();
        assert_eq!(decision.to, ItemStatus::NeedsClarification);
        assert_eq!(task.status(), ItemStatus::NeedsClarification);
        assert_eq!(task.session_ids.clarifier, None);
    }

    #[test]
    fn apply_escalation_records_report() {
        let mut task = crate::Task::new("t");
        task.set_status_for_tests(ItemStatus::CaptainReviewing);
        let decision = apply_escalation(&mut task, Some("boom".into())).unwrap();
        assert_eq!(decision.to, ItemStatus::Escalated);
        assert_eq!(task.escalation_report.as_deref(), Some("boom"));
    }

    #[test]
    fn infer_transition_command_allows_merge_spawn_self_transition() {
        assert_eq!(
            infer_transition_command(ItemStatus::CaptainMerging, ItemStatus::CaptainMerging)
                .unwrap(),
            "merge_spawn"
        );
    }

    // Stop edges: InProgress can reach Stopped, and Stopped can resume
    // (InProgress) or fall back to the queue. These pair with reopen's
    // apply_transition calls in action_contract::reopen so a Stopped task
    // reopens on the same worktree the same way Errored/HandedOff do.
    #[test]
    fn infer_transition_command_allows_stop_from_in_progress() {
        assert_eq!(
            infer_transition_command(ItemStatus::InProgress, ItemStatus::Stopped).unwrap(),
            "stop"
        );
    }

    #[test]
    fn infer_transition_command_allows_resume_from_stopped() {
        assert_eq!(
            infer_transition_command(ItemStatus::Stopped, ItemStatus::InProgress).unwrap(),
            "resume_worker"
        );
        assert_eq!(
            infer_transition_command(ItemStatus::Stopped, ItemStatus::Queued).unwrap(),
            "reopen_queued"
        );
    }

    #[test]
    fn apply_manual_stop_requires_in_progress() {
        assert_eq!(
            apply_manual_transition(ItemStatus::InProgress, TaskLifecycleCommand::Stop).unwrap(),
            ItemStatus::Stopped
        );
        for bad in [
            ItemStatus::Queued,
            ItemStatus::AwaitingReview,
            ItemStatus::HandedOff,
            ItemStatus::Merged,
            ItemStatus::Canceled,
            ItemStatus::Stopped,
        ] {
            assert!(
                apply_manual_transition(bad, TaskLifecycleCommand::Stop).is_err(),
                "stop from {bad:?} must be rejected"
            );
        }
    }

    #[test]
    fn stopped_is_not_finalized_and_is_reopenable() {
        assert!(
            !ItemStatus::Stopped.is_finalized(),
            "Stopped must stay reopenable; finalized statuses are terminal"
        );
        assert!(
            crate::types::REOPENABLE.contains(&ItemStatus::Stopped),
            "Stopped belongs in the REOPENABLE set so reopen_item accepts it"
        );
    }

    #[test]
    fn completed_no_pr_can_be_reopened_for_follow_up_work() {
        assert_eq!(
            infer_transition_command(ItemStatus::CompletedNoPr, ItemStatus::InProgress).unwrap(),
            "resume_worker"
        );
        assert!(
            crate::types::REOPENABLE.contains(&ItemStatus::CompletedNoPr),
            "CompletedNoPr belongs in the REOPENABLE set so no-PR tasks can accept follow-up work"
        );
    }

    #[test]
    fn canceled_can_be_reopened_for_follow_up_work() {
        assert_eq!(
            infer_transition_command(ItemStatus::Canceled, ItemStatus::InProgress).unwrap(),
            "resume_worker"
        );
        assert_eq!(
            infer_transition_command(ItemStatus::Canceled, ItemStatus::Queued).unwrap(),
            "reopen_queued"
        );
        assert!(
            crate::types::REOPENABLE.contains(&ItemStatus::Canceled),
            "Canceled belongs in the REOPENABLE set so canceled tasks can be rerun"
        );
    }
}
