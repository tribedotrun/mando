//! Types for the captain decision journal.

use serde::{Deserialize, Serialize};

/// Curated snapshot of worker state at decision time.
///
/// Key fields from `WorkerContext` that drive classification,
/// minus the large/noisy ones (stream_tail, pr_body, changed_files).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct StateSnapshot {
    pub process_alive: bool,
    pub stream_stale_s: Option<f64>,
    pub seconds_active: f64,
    pub intervention_count: i64,
    pub nudge_count: i64,
    pub no_pr: bool,
    pub reopen_seq: i64,
    pub has_reopen_ack: bool,
    pub branch_ahead: bool,
    pub unresolved_threads: i64,
    pub unreplied_threads: i64,
    pub unaddressed_issue_comments: i64,
    pub pr_ci_status: Option<String>,
}

impl StateSnapshot {
    /// Build from a `WorkerContext` + current nudge count from health state.
    pub(crate) fn from_worker_context(ctx: &mando_types::WorkerContext, nudge_count: i64) -> Self {
        Self {
            process_alive: ctx.process_alive,
            stream_stale_s: ctx.stream_stale_s,
            seconds_active: ctx.seconds_active,
            intervention_count: ctx.intervention_count,
            nudge_count,
            no_pr: ctx.no_pr,
            reopen_seq: ctx.reopen_seq,
            has_reopen_ack: ctx.has_reopen_ack,
            branch_ahead: ctx.branch_ahead,
            unresolved_threads: ctx.unresolved_threads,
            unreplied_threads: ctx.unreplied_threads,
            unaddressed_issue_comments: ctx.unaddressed_issue_comments,
            pr_ci_status: ctx.pr_ci_status.clone(),
        }
    }
}

/// How a decision was classified.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DecisionSource {
    Deterministic,
    Llm,
}

impl std::fmt::Display for DecisionSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Deterministic => write!(f, "deterministic"),
            Self::Llm => write!(f, "llm"),
        }
    }
}

/// Outcome of a past decision, resolved retroactively.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Outcome {
    /// Worker recovered (next tick was Skip).
    Success,
    /// Worker did not recover (same or escalated action next tick).
    Failure,
    /// Worker left the system (item finished/failed/cancelled).
    Terminal,
}

impl std::fmt::Display for Outcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Success => write!(f, "success"),
            Self::Failure => write!(f, "failure"),
            Self::Terminal => write!(f, "terminal"),
        }
    }
}

impl Outcome {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "success" => Some(Self::Success),
            "failure" => Some(Self::Failure),
            "terminal" => Some(Self::Terminal),
            _ => None,
        }
    }
}

/// Input for logging a new decision.
pub struct DecisionInput<'a> {
    pub tick_id: &'a str,
    pub worker: &'a str,
    pub item_id: Option<&'a str>,
    pub action: &'a str,
    pub source: DecisionSource,
    pub rule: &'a str,
    pub state: &'a StateSnapshot,
}

/// A single decision entry in the journal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionEntry {
    pub id: i64,
    pub tick_id: String,
    pub worker: String,
    pub item_id: Option<String>,
    pub action: String,
    pub source: String,
    pub rule: String,
    pub state: StateSnapshot,
    pub outcome: Option<String>,
    pub resolved_at: Option<String>,
    pub created_at: String,
}

/// A distilled pattern from the journal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pattern {
    pub id: i64,
    pub pattern: String,
    pub signal: String,
    pub recommendation: String,
    pub confidence: f64,
    pub sample_size: i64,
    pub status: String,
    pub created_at: String,
}

/// Aggregated stats for the pattern distiller.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionRuleStats {
    pub action: String,
    pub rule: String,
    pub total: i64,
    pub successes: i64,
    pub failures: i64,
    pub success_rate: f64,
}
