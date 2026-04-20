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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_unknown_project_matches_legacy_shape() {
        let err = ScoutError::UnknownProject("atlas".into());
        assert_eq!(err.to_string(), "unknown project 'atlas'");
    }

    #[test]
    fn display_invalid_transition_matches_legacy_shape() {
        let err = ScoutError::InvalidTransition {
            command: "save",
            status: "pending",
        };
        assert_eq!(
            err.to_string(),
            "cannot apply save to scout item in pending"
        );
    }

    #[test]
    fn find_scout_error_survives_context_wrapping() {
        let anyhow_err = anyhow::Error::new(ScoutError::NotFound(42)).context("act on item");
        let typed = find_scout_error(&anyhow_err).expect("should still find typed error");
        assert!(typed.is_not_found());
    }

    #[test]
    fn is_client_error_classification() {
        assert!(ScoutError::UnknownProject("x".into()).is_client_error());
        assert!(ScoutError::NotFound(1).is_client_error());
        assert!(ScoutError::InvalidTransition {
            command: "save",
            status: "pending"
        }
        .is_client_error());
    }
}
