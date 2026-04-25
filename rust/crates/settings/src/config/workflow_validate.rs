//! Startup validation for workflow YAML templates.
//!
//! Ensures all required prompt/nudge keys exist at gateway startup.

use super::workflow::{AgentConfig, CaptainWorkflow, ScoutWorkflow};
use global_claude::CcStreamSymptom;

/// Every `CcStreamSymptom` variant the compiled binary routes on. A user
/// workflow override must declare a rule for each variant — missing a variant
/// would silently disable broken-session detection for that failure mode.
/// Keep this list in sync with the enum.
const REQUIRED_STREAM_SYMPTOMS: &[CcStreamSymptom] = &[
    CcStreamSymptom::ImageDimensionLimit,
    CcStreamSymptom::StreamIdleTimeout,
    CcStreamSymptom::RateLimitAborted,
    CcStreamSymptom::IsError,
    CcStreamSymptom::ContextLengthExceeded,
    CcStreamSymptom::NoConversationFound,
    CcStreamSymptom::SessionInterrupted,
];

/// Required prompt keys for captain workflow.
const REQUIRED_CAPTAIN_PROMPTS: &[&str] = &[
    "worker_initial",
    "worker_briefed",
    "worker_continue",
    "clarifier",
    "interactive_clarifier",
    "captain_review",
    "rebase_worker",
    "task_ask",
    "reopen_resume",
    "review_reopen_message",
    "captain_merge",
    "todo_parse",
];

/// Required nudge keys for captain workflow.
const REQUIRED_CAPTAIN_NUDGES: &[&str] = &[
    "unresolved_threads",
    "missing_work_summary",
    "missing_evidence",
    "stale_evidence",
    "stale_work_summary",
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

/// Check required keys exist in a template map and collect syntax errors.
fn validate_template_map(
    scope: &str,
    required: &[&str],
    templates: &std::collections::HashMap<String, String>,
    errors: &mut Vec<String>,
) {
    for key in required {
        if !templates.contains_key(*key) {
            errors.push(format!("missing: {scope}.{key}"));
        }
    }
    collect_template_errors(scope, templates, errors);
}

/// Validate that a captain workflow has all required template keys and valid syntax.
/// Panics on any errors — call at startup to fail fast.
pub fn validate_captain_workflow(wf: &CaptainWorkflow) {
    let mut errors = Vec::new();
    validate_template_map(
        "prompts",
        REQUIRED_CAPTAIN_PROMPTS,
        &wf.prompts,
        &mut errors,
    );
    validate_template_map("nudges", REQUIRED_CAPTAIN_NUDGES, &wf.nudges, &mut errors);
    validate_template_map(
        "initial_prompts",
        REQUIRED_CAPTAIN_INITIAL_PROMPTS,
        &wf.initial_prompts,
        &mut errors,
    );
    validate_stream_symptoms(&wf.stream_symptoms, &mut errors);
    if !errors.is_empty() {
        global_infra::unrecoverable!(format!(
            "captain workflow missing required template keys: {}",
            errors.join(", ")
        ));
    }
}

/// Reject a workflow whose `stream_symptoms` omits any variant the binary
/// routes on. A missing rule would silently disable broken-session detection
/// for that failure mode — the exact regression a user override could
/// introduce by copying an older captain-workflow.yaml.
fn validate_stream_symptoms(rules: &[global_claude::StreamSymptomRule], errors: &mut Vec<String>) {
    if rules.is_empty() {
        errors.push(
            "stream_symptoms: missing or empty — broken-session detection would be disabled".into(),
        );
        return;
    }
    for required in REQUIRED_STREAM_SYMPTOMS {
        if !rules.iter().any(|r| r.name == *required) {
            errors.push(format!("stream_symptoms: missing rule for {:?}", required));
        }
    }
}

/// Validate that a scout workflow has all required template keys and valid syntax.
/// Panics via `unrecoverable!` on any errors — call at startup to fail fast.
pub fn validate_scout_workflow(wf: &ScoutWorkflow) {
    let mut errors = Vec::new();
    validate_template_map("prompts", REQUIRED_SCOUT_PROMPTS, &wf.prompts, &mut errors);
    if !errors.is_empty() {
        global_infra::unrecoverable!(format!(
            "scout workflow missing required template keys: {}",
            errors.join(", ")
        ));
    }
}

/// Validate timing invariants and positive-value constraints on `AgentConfig`.
/// Returns `Err` with a human-readable message listing all violations.
pub fn try_validate_agent_config(agent: &AgentConfig, tick_interval_s: u64) -> Result<(), String> {
    let mut errors = Vec::new();

    if agent.max_concurrent == 0 {
        errors.push("max_concurrent must be > 0".into());
    }
    if agent.max_interventions == 0 {
        errors.push("max_interventions must be > 0".into());
    }
    if agent.stale_threshold_s.is_zero() {
        errors.push("stale_threshold_s must be > 0".into());
    }
    if agent.worker_timeout_s.is_zero() {
        errors.push("worker_timeout_s must be > 0".into());
    }
    if agent.captain_review_timeout_s.is_zero() {
        errors.push("captain_review_timeout_s must be > 0".into());
    }

    // Relative checks only when individual values are positive.
    if !agent.worker_timeout_s.is_zero()
        && !agent.stale_threshold_s.is_zero()
        && agent.worker_timeout_s <= agent.stale_threshold_s
    {
        errors.push(format!(
            "worker_timeout_s ({}s) must be > stale_threshold_s ({}s)",
            agent.worker_timeout_s.as_secs_f64(),
            agent.stale_threshold_s.as_secs_f64()
        ));
    }

    let min_stale = std::time::Duration::from_secs(2 * tick_interval_s);
    if !agent.stale_threshold_s.is_zero() && agent.stale_threshold_s < min_stale {
        errors.push(format!(
            "stale_threshold_s ({}s) must be >= 2 * tick_interval_s ({}s)",
            agent.stale_threshold_s.as_secs_f64(),
            min_stale.as_secs_f64()
        ));
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "agent config validation failed: {}",
            errors.join(", ")
        ))
    }
}

/// Panicking wrapper for startup — delegates to `try_validate_agent_config`.
pub fn validate_agent_config(agent: &AgentConfig, tick_interval_s: u64) {
    if let Err(msg) = try_validate_agent_config(agent, tick_interval_s) {
        global_infra::unrecoverable!(msg);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn default_agent() -> AgentConfig {
        AgentConfig::default()
    }

    #[test]
    fn default_agent_config_is_valid() {
        // Default tick_interval_s = 30, default stale_threshold_s = 1200 → 1200 >= 60 ✓
        validate_agent_config(&default_agent(), 30);
    }

    #[test]
    #[should_panic(expected = "max_concurrent must be > 0")]
    fn zero_max_concurrent_panics() {
        let mut ac = default_agent();
        ac.max_concurrent = 0;
        validate_agent_config(&ac, 30);
    }

    #[test]
    #[should_panic(expected = "max_interventions must be > 0")]
    fn zero_max_interventions_panics() {
        let mut ac = default_agent();
        ac.max_interventions = 0;
        validate_agent_config(&ac, 30);
    }

    #[test]
    #[should_panic(expected = "worker_timeout_s")]
    fn worker_timeout_not_greater_than_stale_panics() {
        use std::time::Duration;
        let mut ac = default_agent();
        ac.worker_timeout_s = Duration::from_secs(100);
        ac.stale_threshold_s = Duration::from_secs(100); // equal, not greater
        validate_agent_config(&ac, 30);
    }

    #[test]
    #[should_panic(expected = "stale_threshold_s")]
    fn stale_threshold_below_2x_tick_panics() {
        use std::time::Duration;
        let mut ac = default_agent();
        ac.stale_threshold_s = Duration::from_secs(50); // < 2 * 30 = 60
        ac.worker_timeout_s = Duration::from_secs(21600);
        validate_agent_config(&ac, 30);
    }

    #[test]
    #[should_panic(expected = "captain_review_timeout_s must be > 0")]
    fn zero_captain_review_timeout_panics() {
        let mut ac = default_agent();
        ac.captain_review_timeout_s = std::time::Duration::ZERO;
        validate_agent_config(&ac, 30);
    }

    #[test]
    fn multiple_errors_reported_together() {
        let mut ac = default_agent();
        ac.max_concurrent = 0;
        ac.max_interventions = 0;
        let result = std::panic::catch_unwind(|| validate_agent_config(&ac, 30));
        let err = result.unwrap_err();
        let msg = err.downcast_ref::<String>().unwrap();
        assert!(msg.contains("max_concurrent"));
        assert!(msg.contains("max_interventions"));
    }

    #[test]
    fn validate_stream_symptoms_rejects_empty_list() {
        let mut errors = Vec::new();
        validate_stream_symptoms(&[], &mut errors);
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("missing or empty"), "got: {:?}", errors);
    }

    #[test]
    fn validate_stream_symptoms_accepts_compiled_default() {
        let wf = CaptainWorkflow::compiled_default();
        let mut errors = Vec::new();
        validate_stream_symptoms(&wf.stream_symptoms, &mut errors);
        assert!(errors.is_empty(), "compiled default failed: {:?}", errors);
    }

    #[test]
    fn validate_stream_symptoms_reports_each_missing_variant() {
        // Strip a couple of variants and confirm both are named in the
        // error list — users who copy-paste an older yaml need to know
        // exactly which rules to restore.
        let wf = CaptainWorkflow::compiled_default();
        let kept: Vec<_> = wf
            .stream_symptoms
            .into_iter()
            .filter(|r| {
                r.name != CcStreamSymptom::SessionInterrupted
                    && r.name != CcStreamSymptom::NoConversationFound
            })
            .collect();
        let mut errors = Vec::new();
        validate_stream_symptoms(&kept, &mut errors);
        let joined = errors.join(" | ");
        assert!(joined.contains("SessionInterrupted"), "got: {joined}");
        assert!(joined.contains("NoConversationFound"), "got: {joined}");
    }
}
