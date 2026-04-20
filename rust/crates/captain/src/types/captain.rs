//! Captain domain types — worker context, tick results, and actions.

use std::collections::HashMap;
use std::fmt;

use serde::{Deserialize, Serialize};

/// Context gathered for a single worker during captain tick.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct WorkerContext {
    pub session_name: String,
    pub item_title: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pr: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pr_ci_status: Option<String>,
    pub pr_comments: i64,
    pub unresolved_threads: i64,
    pub unreplied_threads: i64,
    pub unaddressed_issue_comments: i64,
    pub pr_body: String,
    pub changed_files: Vec<String>,
    pub branch_ahead: bool,
    pub process_alive: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cpu_time_s: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prev_cpu_time_s: Option<f64>,
    pub stream_tail: String,
    pub seconds_active: f64,
    pub intervention_count: i64,
    pub no_pr: bool,
    pub reopen_seq: i64,
    pub has_reopen_ack: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reopen_source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stream_stale_s: Option<f64>,
    pub pr_head_sha: String,
    /// True when some context data could not be fetched (API errors, timeouts).
    /// Captain LLM should be conservative when degraded -- prefer skip over action.
    pub degraded: bool,

    // ── DB-backed artifact gates (populated from task_artifacts table) ──
    /// True when at least one evidence artifact exists in DB.
    pub has_evidence: bool,
    /// True when evidence is fresh (reopen_seq == 0, or latest evidence created_at > reopened_at).
    pub evidence_fresh: bool,
    /// True when at least one work_summary artifact exists in DB.
    pub has_work_summary: bool,
    /// True when work summary is fresh (same logic as evidence).
    pub work_summary_fresh: bool,
    /// True when evidence contains at least one screenshot (png/jpg/jpeg/webp).
    pub has_screenshot: bool,
    /// True when evidence contains at least one recording (gif/mp4/mov/webm).
    pub has_recording: bool,
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
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TickMode {
    #[default]
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
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TickResult {
    pub mode: TickMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tick_id: Option<String>,
    pub max_workers: usize,
    pub active_workers: usize,
    pub tasks: HashMap<String, usize>,
    pub alerts: Vec<String>,
    pub dry_actions: Vec<Action>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// True when the tick ran during a rate-limit cooldown (spawning was suppressed).
    pub rate_limited: bool,
}
