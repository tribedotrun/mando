//! Typed errors for task creation. Route handlers downcast on these
//! variants instead of string-matching formatted error messages.

use std::fmt;

/// Task-creation failure that transport-http maps onto a 4xx status.
#[derive(Debug)]
#[non_exhaustive]
pub enum TaskCreateError {
    /// No projects configured — user must add one before creating tasks.
    NoProjectConfigured,
    /// Projects exist but caller did not select one and no prefix matched.
    ProjectSelectionRequired,
    /// Caller specified a project name that does not resolve.
    UnknownProject { name: String, valid: Vec<String> },
}

impl fmt::Display for TaskCreateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoProjectConfigured => {
                f.write_str("no project configured — add a project before creating tasks")
            }
            Self::ProjectSelectionRequired => {
                f.write_str("project selection required — choose a project before creating tasks")
            }
            Self::UnknownProject { name, valid } => {
                let valid_list = if valid.is_empty() {
                    "(none configured)".to_string()
                } else {
                    valid.join(", ")
                };
                write!(f, "unknown project {name:?} — valid projects: {valid_list}")
            }
        }
    }
}

impl std::error::Error for TaskCreateError {}

/// Walk the anyhow chain looking for a typed task-creation error.
pub fn find_task_create_error(err: &anyhow::Error) -> Option<&TaskCreateError> {
    err.chain()
        .find_map(|src| src.downcast_ref::<TaskCreateError>())
}
