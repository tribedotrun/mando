//! Typed scout errors. Route handlers downcast on these variants instead of
//! string-matching formatted error messages.

use std::fmt;

/// Typed error for scout runtime operations that route handlers need to map
/// onto specific HTTP status codes.
#[derive(Debug)]
#[non_exhaustive]
pub enum ScoutError {
    /// The `project` passed to `act_on_item` is not in the config.
    UnknownProject(String),
    /// No scout item with the given id.
    NotFound(i64),
    /// Lifecycle command is not applicable to the item's current status.
    InvalidTransition {
        command: &'static str,
        status: &'static str,
    },
    /// Telegraph publish requested on an item that has no article body yet.
    NoArticleContent(i64),
}

impl fmt::Display for ScoutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownProject(name) => write!(f, "unknown project '{name}'"),
            Self::NotFound(id) => write!(f, "scout item #{id} not found"),
            Self::InvalidTransition { command, status } => {
                write!(f, "cannot apply {command} to scout item in {status}")
            }
            Self::NoArticleContent(id) => write!(
                f,
                "scout item #{id} has no article content — needs processing"
            ),
        }
    }
}

impl std::error::Error for ScoutError {}

impl ScoutError {
    /// Client-facing errors map to 4xx; anything else is 5xx.
    pub fn is_client_error(&self) -> bool {
        matches!(
            self,
            Self::UnknownProject(_)
                | Self::NotFound(_)
                | Self::InvalidTransition { .. }
                | Self::NoArticleContent(_)
        )
    }

    pub fn is_not_found(&self) -> bool {
        matches!(self, Self::NotFound(_) | Self::NoArticleContent(_))
    }
}

/// Walk the anyhow chain looking for a typed scout error so callers survive
/// upstream `.context(..)` additions without string-matching.
pub fn find_scout_error(err: &anyhow::Error) -> Option<&ScoutError> {
    err.chain().find_map(|src| src.downcast_ref::<ScoutError>())
}
