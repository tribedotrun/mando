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
    ExhaustionReport,
    TaskAsk,
    ScoutProcess,
    ScoutArticle,
    ScoutQa,
    ScoutResearch,
    ScoutAct,
    VoiceAgent,
}

/// Display group — used for UI category chips and aggregation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CallerGroup {
    Workers,
    Clarifier,
    CaptainReview,
    CaptainOps,
    Scout,
    Voice,
}

impl SessionCaller {
    /// The string stored in SQLite.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Worker => "worker",
            Self::Clarifier => "clarifier",
            Self::DeepClarifier => "deep-clarifier",
            Self::CaptainReviewAsync => "captain-review-async",
            Self::ExhaustionReport => "exhaustion-report",
            Self::TaskAsk => "task-ask",
            Self::ScoutProcess => "scout-process",
            Self::ScoutArticle => "scout-article",
            Self::ScoutQa => "scout-qa",
            Self::ScoutResearch => "scout-research",
            Self::ScoutAct => "scout-act",
            Self::VoiceAgent => "voice-agent",
        }
    }

    /// Parse from the string stored in SQLite.
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "worker" => Some(Self::Worker),
            "clarifier" => Some(Self::Clarifier),
            "deep-clarifier" => Some(Self::DeepClarifier),
            "captain-review-async" => Some(Self::CaptainReviewAsync),
            "exhaustion-report" => Some(Self::ExhaustionReport),
            "task-ask" => Some(Self::TaskAsk),
            "scout-process" => Some(Self::ScoutProcess),
            "scout-article" => Some(Self::ScoutArticle),
            "scout-qa" => Some(Self::ScoutQa),
            "scout-research" => Some(Self::ScoutResearch),
            "scout-act" => Some(Self::ScoutAct),
            "voice-agent" => Some(Self::VoiceAgent),
            _ => None,
        }
    }

    /// Which display group this caller belongs to.
    pub fn group(&self) -> CallerGroup {
        match self {
            Self::Worker => CallerGroup::Workers,
            Self::Clarifier | Self::DeepClarifier => CallerGroup::Clarifier,
            Self::CaptainReviewAsync => CallerGroup::CaptainReview,
            Self::ExhaustionReport | Self::TaskAsk => CallerGroup::CaptainOps,
            Self::ScoutProcess
            | Self::ScoutArticle
            | Self::ScoutQa
            | Self::ScoutResearch
            | Self::ScoutAct => CallerGroup::Scout,
            Self::VoiceAgent => CallerGroup::Voice,
        }
    }

    /// All known callers, in display order.
    pub fn all() -> &'static [Self] {
        &[
            Self::Worker,
            Self::Clarifier,
            Self::DeepClarifier,
            Self::CaptainReviewAsync,
            Self::ExhaustionReport,
            Self::TaskAsk,
            Self::ScoutProcess,
            Self::ScoutArticle,
            Self::ScoutQa,
            Self::ScoutResearch,
            Self::ScoutAct,
            Self::VoiceAgent,
        ]
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
            Self::Voice => "voice",
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
        assert!(!SessionCaller::VoiceAgent.requires_scout_item());
    }
}
