//! Startup validation for workflow YAML templates.
//!
//! Ensures all required prompt/nudge keys exist at gateway startup.

use super::workflow::{CaptainWorkflow, ScoutWorkflow};

/// Required prompt keys for captain workflow.
const REQUIRED_CAPTAIN_PROMPTS: &[&str] = &[
    "worker_initial",
    "worker_briefed",
    "worker_continue",
    "clarifier",
    "deep_clarifier",
    "interactive_clarifier",
    "captain_review",
    "rebase_worker",
    "pattern_distiller",
    "task_analyst",
    "guardian_repair",
    "restart_resume",
    "reopen_resume",
    "review_reopen_message",
    "captain_merge",
];

/// Required nudge keys for captain workflow.
const REQUIRED_CAPTAIN_NUDGES: &[&str] = &[
    "continuation_preamble",
    "unresolved_threads",
    "missing_diagram",
    "missing_evidence",
    "stream_stale",
    "image_dimension_blocked",
    "reopen_ack",
    "nudge_default",
    "nopr_insufficient_output",
];

/// Required initial prompt keys for captain workflow.
const REQUIRED_CAPTAIN_INITIAL_PROMPTS: &[&str] = &["worker", "adopted"];

/// Required prompt keys for scout workflow.
const REQUIRED_SCOUT_PROMPTS: &[&str] = &["process", "synthesize", "qa", "research", "act"];

/// Validate that a captain workflow has all required template keys and valid syntax.
/// Panics on any errors — call at startup to fail fast.
pub fn validate_captain_workflow(wf: &CaptainWorkflow) {
    let mut errors = Vec::new();
    for key in REQUIRED_CAPTAIN_PROMPTS {
        if !wf.prompts.contains_key(*key) {
            errors.push(format!("missing: prompts.{key}"));
        }
    }
    for key in REQUIRED_CAPTAIN_NUDGES {
        if !wf.nudges.contains_key(*key) {
            errors.push(format!("missing: nudges.{key}"));
        }
    }
    for key in REQUIRED_CAPTAIN_INITIAL_PROMPTS {
        if !wf.initial_prompts.contains_key(*key) {
            errors.push(format!("missing: initial_prompts.{key}"));
        }
    }
    collect_template_errors("captain.prompts", &wf.prompts, &mut errors);
    collect_template_errors("captain.nudges", &wf.nudges, &mut errors);
    collect_template_errors("captain.initial_prompts", &wf.initial_prompts, &mut errors);
    if !errors.is_empty() {
        panic!(
            "captain workflow missing required template keys: {}",
            errors.join(", ")
        );
    }
}

/// Validate that a scout workflow has all required template keys and valid syntax.
/// Panics on any errors — call at startup to fail fast.
pub fn validate_scout_workflow(wf: &ScoutWorkflow) {
    let mut errors = Vec::new();
    for key in REQUIRED_SCOUT_PROMPTS {
        if !wf.prompts.contains_key(*key) {
            errors.push(format!("missing: prompts.{key}"));
        }
    }
    collect_template_errors("scout.prompts", &wf.prompts, &mut errors);
    if !errors.is_empty() {
        panic!(
            "scout workflow missing required template keys: {}",
            errors.join(", ")
        );
    }
}

fn collect_template_errors(
    scope: &str,
    templates: &std::collections::HashMap<String, String>,
    errors: &mut Vec<String>,
) {
    for (name, template) in templates {
        if let Err(err) = super::workflow::validate_template_syntax(template) {
            errors.push(format!("syntax: {scope}.{name}: {err}"));
        }
    }
}
