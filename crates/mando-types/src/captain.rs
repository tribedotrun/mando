//! Captain domain types — worker context, tick results, and actions.

use std::collections::HashMap;
use std::fmt;

use serde::{Deserialize, Serialize};

/// Context gathered for a single worker during captain tick.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerContext {
    pub session_name: String,
    pub item_title: String,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pr: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pr_ci_status: Option<String>,
    #[serde(default)]
    pub pr_comments: i64,
    #[serde(default)]
    pub unresolved_threads: i64,
    #[serde(default)]
    pub unreplied_threads: i64,
    #[serde(default)]
    pub unaddressed_issue_comments: i64,
    #[serde(default)]
    pub pr_body: String,
    #[serde(default)]
    pub changed_files: Vec<String>,
    #[serde(default)]
    pub branch_ahead: bool,
    #[serde(default)]
    pub process_alive: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpu_time_s: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prev_cpu_time_s: Option<f64>,
    #[serde(default)]
    pub stream_tail: String,
    #[serde(default)]
    pub seconds_active: f64,
    #[serde(default)]
    pub intervention_count: i64,
    #[serde(default)]
    pub no_pr: bool,
    #[serde(default)]
    pub reopen_seq: i64,
    #[serde(default)]
    pub has_reopen_ack: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reopen_source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream_stale_s: Option<f64>,
    #[serde(default)]
    pub pr_head_sha: String,
    /// True when some context data could not be fetched (API errors, timeouts).
    /// Captain LLM should be conservative when degraded — prefer skip over action.
    #[serde(default)]
    pub degraded: bool,
}

/// The kind of action the captain can take on a worker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ActionKind {
    #[serde(rename = "skip")]
    Skip,
    #[serde(rename = "nudge")]
    Nudge,
    #[serde(rename = "captain-review")]
    CaptainReview,
    #[serde(rename = "ship")]
    Ship,
}

/// A captain action targeting a specific worker.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Action {
    pub worker: String,
    pub action: ActionKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// The execution mode of a captain tick.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TickMode {
    Live,
    DryRun,
    Skipped,
}

impl fmt::Display for TickMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Live => write!(f, "live"),
            Self::DryRun => write!(f, "dry-run"),
            Self::Skipped => write!(f, "skipped"),
        }
    }
}

/// Structured result from a captain tick.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TickResult {
    pub mode: TickMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tick_id: Option<String>,
    #[serde(default)]
    pub max_workers: usize,
    #[serde(default)]
    pub active_workers: usize,
    #[serde(default)]
    pub tasks: HashMap<String, usize>,
    #[serde(default)]
    pub alerts: Vec<String>,
    #[serde(default)]
    pub dry_actions: Vec<Action>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// True when the tick ran during a rate-limit cooldown (spawning was suppressed).
    #[serde(default)]
    pub rate_limited: bool,
}
