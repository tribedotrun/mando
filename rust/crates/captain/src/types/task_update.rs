//! Typed errors and update struct for task field updates (PATCH).

use std::fmt;

use super::session_ids::SessionIds;
pub use super::task_status::ReviewTrigger;

/// Typed error for task field updates, replaces string-based error classification.
#[derive(Debug)]
#[non_exhaustive]
pub enum TaskUpdateError {
    NotFound(i64),
    InvalidStatus(String),
    InvalidFieldType {
        field: String,
        expected: &'static str,
    },
    InvalidBooleanValue {
        field: String,
        value: String,
    },
    UnknownField(String),
    FieldCannotBeNull(String),
    TerminalStatusTransition(String),
    LifecycleFieldCannotBePatched(String),
    NotAnObject,
}

impl fmt::Display for TaskUpdateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(id) => write!(f, "task not found: {id}"),
            Self::InvalidStatus(s) => write!(f, "invalid status: {s}"),
            Self::InvalidFieldType { field, expected } => {
                write!(f, "invalid field type for {field}: expected {expected}")
            }
            Self::InvalidBooleanValue { field, value } => {
                write!(f, "invalid boolean value for {field}: {value}")
            }
            Self::UnknownField(s) => write!(f, "unknown field: {s}"),
            Self::FieldCannotBeNull(s) => write!(f, "field {s} cannot be null"),
            Self::TerminalStatusTransition(s) => {
                write!(f, "cannot transition from terminal status {s}")
            }
            Self::LifecycleFieldCannotBePatched(s) => {
                write!(
                    f,
                    "field {s} must use a lifecycle command, not a generic patch"
                )
            }
            Self::NotAnObject => write!(f, "updates body must be a JSON object"),
        }
    }
}

impl std::error::Error for TaskUpdateError {}

impl TaskUpdateError {
    pub fn is_client_error(&self) -> bool {
        !matches!(self, Self::NotFound(_))
    }

    pub fn is_not_found(&self) -> bool {
        matches!(self, Self::NotFound(_))
    }
}

/// Typed patch struct for task field updates.
///
/// Each field has outer `Option` semantics:
/// - `None`  → field is untouched (not in the patch)
/// - `Some(v)` → field is set to `v`
///
/// For nullable task fields the inner value is also `Option`:
/// - `Some(None)` → field is cleared (set to NULL)
/// - `Some(Some(v))` → field is set to `v`
///
/// Lifecycle fields (`status`, `planning`) are intentionally absent —
/// they must go through the lifecycle API.
#[derive(Debug, Default, Clone)]
pub struct UpdateTaskInput {
    pub title: Option<String>,
    pub project_id: Option<i64>,
    pub worker: Option<Option<String>>,
    pub resource: Option<Option<String>>,
    pub context: Option<Option<String>>,
    pub original_prompt: Option<Option<String>>,
    pub created_at: Option<Option<String>>,
    pub workbench_id: Option<i64>,
    pub pr_number: Option<Option<i64>>,
    pub worker_started_at: Option<Option<String>>,
    pub intervention_count: Option<i64>,
    pub captain_review_trigger: Option<Option<ReviewTrigger>>,
    pub last_activity_at: Option<Option<String>>,
    pub plan: Option<Option<String>>,
    pub no_pr: Option<bool>,
    pub no_auto_merge: Option<bool>,
    pub is_bug_fix: Option<bool>,
    pub worker_seq: Option<i64>,
    pub reopen_seq: Option<i64>,
    pub reopened_at: Option<Option<String>>,
    pub reopen_source: Option<Option<String>>,
    pub images: Option<Option<String>>,
    pub review_fail_count: Option<i64>,
    pub clarifier_fail_count: Option<i64>,
    pub spawn_fail_count: Option<i64>,
    pub merge_fail_count: Option<i64>,
    pub escalation_report: Option<Option<String>>,
    pub source: Option<Option<String>>,
    pub session_ids: Option<SessionIds>,
}
