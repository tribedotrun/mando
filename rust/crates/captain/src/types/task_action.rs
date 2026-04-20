//! Typed errors for task lifecycle-action commands (queue, accept, cancel,
//! rework, handoff, start-merge, retry-review). Route handlers downcast on
//! these variants instead of string-matching formatted error messages.

use std::fmt;

/// Task-action failure that transport-http maps onto the correct HTTP status.
#[derive(Debug)]
#[non_exhaustive]
pub enum TaskActionError {
    /// No task with the given id.
    NotFound(i64),
    /// A lifecycle command is not permitted from the task's current status.
    InvalidTransition {
        command: &'static str,
        status: &'static str,
    },
    /// Cancel was requested against an already-finalized task.
    FinalizedState(&'static str),
    /// Row update saw 0 rows affected — someone else raced the same task.
    Conflict { message: String },
}

impl fmt::Display for TaskActionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(id) => write!(f, "task not found: {id}"),
            Self::InvalidTransition { command, status } => {
                write!(f, "cannot {command} task from {status}")
            }
            Self::FinalizedState(status) => {
                write!(f, "cannot cancel task from finalized state {status}")
            }
            Self::Conflict { message } => f.write_str(message),
        }
    }
}

impl std::error::Error for TaskActionError {}

impl TaskActionError {
    pub fn is_not_found(&self) -> bool {
        matches!(self, Self::NotFound(_))
    }

    pub fn is_conflict(&self) -> bool {
        matches!(
            self,
            Self::InvalidTransition { .. } | Self::FinalizedState(_) | Self::Conflict { .. }
        )
    }
}

/// Walk the anyhow chain looking for a typed task-action error so callers
/// survive upstream `.context(..)` additions without string-matching.
pub fn find_task_action_error(err: &anyhow::Error) -> Option<&TaskActionError> {
    err.chain()
        .find_map(|src| src.downcast_ref::<TaskActionError>())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_not_found_matches_legacy_shape() {
        assert_eq!(
            TaskActionError::NotFound(42).to_string(),
            "task not found: 42"
        );
    }

    #[test]
    fn display_invalid_transition_matches_legacy_shape() {
        let err = TaskActionError::InvalidTransition {
            command: "queue",
            status: "merged",
        };
        assert_eq!(err.to_string(), "cannot queue task from merged");
    }

    #[test]
    fn display_finalized_state_matches_legacy_shape() {
        let err = TaskActionError::FinalizedState("merged");
        assert_eq!(
            err.to_string(),
            "cannot cancel task from finalized state merged"
        );
    }

    #[test]
    fn find_task_action_error_survives_context_wrapping() {
        let wrapped =
            anyhow::Error::new(TaskActionError::NotFound(7)).context("http queue handler");
        let typed = find_task_action_error(&wrapped).expect("typed error should be reachable");
        assert!(typed.is_not_found());
    }

    #[test]
    fn classifier_helpers() {
        assert!(TaskActionError::NotFound(1).is_not_found());
        assert!(!TaskActionError::NotFound(1).is_conflict());
        assert!(TaskActionError::FinalizedState("merged").is_conflict());
        assert!(TaskActionError::InvalidTransition {
            command: "queue",
            status: "merged",
        }
        .is_conflict());
    }
}
