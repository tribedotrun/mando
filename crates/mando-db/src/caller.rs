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
    DeepClarifier,
    CaptainReviewAsync,
    CaptainMergeAsync,
    ExhaustionReport,
    TaskAsk,
    ParseTodos,
    ScoutProcess,
    ScoutArticle,
    ScoutQa,
    ScoutResearch,
    ScoutAct,
}

/// Display group — used for UI category chips and aggregation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CallerGroup {
    Workers,
    Clarifier,
    CaptainReview,
    CaptainOps,
    Scout,
}

impl SessionCaller {
    /// The string stored in SQLite.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Worker => "worker",
            Self::Clarifier => "clarifier",
            Self::DeepClarifier => "deep-clarifier",
            Self::CaptainReviewAsync => "captain-review-async",
            Self::CaptainMergeAsync => "captain-merge-async",
            Self::ExhaustionReport => "exhaustion-report",
            Self::TaskAsk => "task-ask",
            Self::ParseTodos => "parse-todos",
            Self::ScoutProcess => "scout-process",
            Self::ScoutArticle => "scout-article",
            Self::ScoutQa => "scout-qa",
            Self::ScoutResearch => "scout-research",
            Self::ScoutAct => "scout-act",
        }
    }

    /// Parse from the string stored in SQLite.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "worker" => Some(Self::Worker),
            "clarifier" => Some(Self::Clarifier),
            "deep-clarifier" => Some(Self::DeepClarifier),
            "captain-review-async" => Some(Self::CaptainReviewAsync),
            "captain-merge-async" => Some(Self::CaptainMergeAsync),
            "exhaustion-report" => Some(Self::ExhaustionReport),
            "task-ask" => Some(Self::TaskAsk),
            "parse-todos" => Some(Self::ParseTodos),
            "scout-process" => Some(Self::ScoutProcess),
            "scout-article" => Some(Self::ScoutArticle),
            "scout-qa" => Some(Self::ScoutQa),
            "scout-research" => Some(Self::ScoutResearch),
            "scout-act" => Some(Self::ScoutAct),
            // Prefixed callers: session key includes an embedded ID but maps
            // to the same logical caller for grouping/display.
            s if s.starts_with("parse-todos-") => Some(Self::ParseTodos),
            s if s.starts_with("task-ask:") => Some(Self::TaskAsk),
            _ => None,
        }
    }

    /// Which display group this caller belongs to.
    pub fn group(&self) -> CallerGroup {
        match self {
            Self::Worker => CallerGroup::Workers,
            Self::Clarifier | Self::DeepClarifier => CallerGroup::Clarifier,
            Self::CaptainReviewAsync => CallerGroup::CaptainReview,
            Self::CaptainMergeAsync | Self::ExhaustionReport | Self::TaskAsk | Self::ParseTodos => {
                CallerGroup::CaptainOps
            }
            Self::ScoutProcess
            | Self::ScoutArticle
            | Self::ScoutQa
            | Self::ScoutResearch
            | Self::ScoutAct => CallerGroup::Scout,
        }
    }

    /// All known callers, in display order.
    pub fn all() -> &'static [Self] {
        &[
            Self::Worker,
            Self::Clarifier,
            Self::DeepClarifier,
            Self::CaptainReviewAsync,
            Self::CaptainMergeAsync,
            Self::ExhaustionReport,
            Self::TaskAsk,
            Self::ParseTodos,
            Self::ScoutProcess,
            Self::ScoutArticle,
            Self::ScoutQa,
            Self::ScoutResearch,
            Self::ScoutAct,
        ]
    }

    /// SQL LIKE prefix for callers that use key-embedded IDs.
    /// Returns `None` for callers stored with their canonical name only.
    pub fn like_prefix(&self) -> Option<&'static str> {
        match self {
            Self::ParseTodos => Some("parse-todos-%"),
            Self::TaskAsk => Some("task-ask:%"),
            _ => None,
        }
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
    fn scout_callers_require_item() {
        assert!(SessionCaller::ScoutProcess.requires_scout_item());
        assert!(SessionCaller::ScoutArticle.requires_scout_item());
        assert!(!SessionCaller::Worker.requires_scout_item());
        assert!(!SessionCaller::CaptainReviewAsync.requires_scout_item());
    }
}
