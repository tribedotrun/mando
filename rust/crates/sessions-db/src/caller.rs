//! Session caller enum — identifies which subsystem spawned a CC session.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Every CC session caller in the system. Stored as the string representation
/// in SQLite and used for display grouping in the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SessionCaller {
    Worker,
    Clarifier,
    CaptainReviewAsync,
    CaptainMergeAsync,
    ScoutProcess,
    ScoutArticle,
    ScoutQa,
    ScoutResearch,
    ScoutAct,
    Rebase,
}

/// Display group — used for UI category chips and aggregation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CallerGroup {
    Workers,
    Clarifier,
    CaptainReview,
    CaptainOps,
    Scout,
    Rebase,
}

impl SessionCaller {
    /// The string stored in SQLite.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Worker => "worker",
            Self::Clarifier => "clarifier",
            Self::CaptainReviewAsync => "captain-review-async",
            Self::CaptainMergeAsync => "captain-merge-async",
            Self::ScoutProcess => "scout-process",
            Self::ScoutArticle => "scout-article",
            Self::ScoutQa => "scout-qa",
            Self::ScoutResearch => "scout-research",
            Self::ScoutAct => "scout-act",
            Self::Rebase => "rebase",
        }
    }

    /// Parse from the string stored in SQLite.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "worker" => Some(Self::Worker),
            "clarifier" => Some(Self::Clarifier),
            "captain-review-async" => Some(Self::CaptainReviewAsync),
            "captain-merge-async" => Some(Self::CaptainMergeAsync),
            "scout-process" => Some(Self::ScoutProcess),
            "scout-article" => Some(Self::ScoutArticle),
            "scout-qa" => Some(Self::ScoutQa),
            "scout-research" => Some(Self::ScoutResearch),
            "scout-act" => Some(Self::ScoutAct),
            "rebase" => Some(Self::Rebase),
            _ => None,
        }
    }

    /// Which display group this caller belongs to.
    pub fn group(&self) -> CallerGroup {
        match self {
            Self::Worker => CallerGroup::Workers,
            Self::Clarifier => CallerGroup::Clarifier,
            Self::CaptainReviewAsync => CallerGroup::CaptainReview,
            Self::CaptainMergeAsync => CallerGroup::CaptainOps,
            Self::ScoutProcess
            | Self::ScoutArticle
            | Self::ScoutQa
            | Self::ScoutResearch
            | Self::ScoutAct => CallerGroup::Scout,
            Self::Rebase => CallerGroup::Rebase,
        }
    }

    /// All known callers, in display order.
    pub fn all() -> &'static [Self] {
        &[
            Self::Worker,
            Self::Clarifier,
            Self::CaptainReviewAsync,
            Self::CaptainMergeAsync,
            Self::ScoutProcess,
            Self::ScoutArticle,
            Self::ScoutQa,
            Self::ScoutResearch,
            Self::ScoutAct,
            Self::Rebase,
        ]
    }

    /// SQL LIKE prefix for callers that use key-embedded IDs.
    /// No current callers use that form.
    pub fn like_prefix(&self) -> Option<&'static str> {
        None
    }

    /// Whether this caller requires a scout_item_id.
    pub fn requires_scout_item(&self) -> bool {
        matches!(
            self,
            Self::ScoutProcess
                | Self::ScoutArticle
                | Self::ScoutQa
                | Self::ScoutResearch
                | Self::ScoutAct
        )
    }
}

impl CallerGroup {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Workers => "workers",
            Self::Clarifier => "clarifier",
            Self::CaptainReview => "captain-review",
            Self::CaptainOps => "captain-ops",
            Self::Scout => "scout",
            Self::Rebase => "rebase",
        }
    }
}

impl fmt::Display for SessionCaller {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Display for CallerGroup {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_all_callers() {
        for caller in SessionCaller::all() {
            let s = caller.as_str();
            let parsed = SessionCaller::parse(s).unwrap_or_else(|| {
                panic!("failed to parse caller: {s}");
            });
            assert_eq!(*caller, parsed);
        }
    }

    #[test]
    fn unknown_callers_degrade_to_none() {
        for caller in ["unknown", "retired-session", "retired-session:42"] {
            assert_eq!(SessionCaller::parse(caller), None);
        }
    }

    #[test]
    fn scout_callers_require_item() {
        assert!(SessionCaller::ScoutProcess.requires_scout_item());
        assert!(SessionCaller::ScoutArticle.requires_scout_item());
        assert!(!SessionCaller::Worker.requires_scout_item());
        assert!(!SessionCaller::CaptainReviewAsync.requires_scout_item());
    }
}
