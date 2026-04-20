//! Typed errors for captain's git helpers. Callers match on variants
//! instead of inspecting git's stderr text.

use std::fmt;

/// Failure modes from [`crate::io::git`] that callers branch on.
#[derive(Debug)]
#[non_exhaustive]
pub enum GitError {
    /// `git worktree add -b <branch>` saw the branch or directory already
    /// exists — caller typically retries in a fresh slot.
    WorktreeAlreadyExists { branch: String },
    /// `git commit` refused because nothing was staged — caller treats as
    /// a no-op rather than a hard failure.
    NothingToCommit,
}

impl fmt::Display for GitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WorktreeAlreadyExists { branch } => {
                write!(f, "git worktree/branch {branch:?} already exists")
            }
            Self::NothingToCommit => f.write_str("nothing to commit"),
        }
    }
}

impl std::error::Error for GitError {}

pub fn find_git_error(err: &anyhow::Error) -> Option<&GitError> {
    err.chain().find_map(|src| src.downcast_ref::<GitError>())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_includes_branch() {
        let err = GitError::WorktreeAlreadyExists {
            branch: "mando/foo-1".into(),
        };
        assert!(err.to_string().contains("mando/foo-1"));
    }

    #[test]
    fn find_survives_context_wrapping() {
        let wrapped = anyhow::Error::new(GitError::NothingToCommit).context("scaffold commit");
        let typed = find_git_error(&wrapped).expect("typed error reachable");
        assert!(matches!(typed, GitError::NothingToCommit));
    }
}
