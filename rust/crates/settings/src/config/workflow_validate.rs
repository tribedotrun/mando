//! Startup validation for workflow YAML templates.
//!
//! Ensures all required prompt/nudge keys exist at gateway startup.

use super::workflow::{
    AgentConfig, CaptainWorkflow, CodexApprovalPolicy, CodexApprovalsReviewer, ScoutWorkflow,
};
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
    "task_ask_reopen_synthesis",
    "advisor",
    "advisor_reopen_synthesis",
    "advisor_reopen_direct",
    "reopen_resume",
    "review_reopen_message",
    "captain_merge",
    "todo_parse",
    "planning_initial",
    "planning_cc_feedback",
    "planning_codex_feedback",
    "planning_synthesize",
    "planning_final",
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
    "gates_incomplete",
];

/// Required initial prompt keys for captain workflow.
const REQUIRED_CAPTAIN_INITIAL_PROMPTS: &[&str] = &["worker", "adopted"];

/// Required prompt keys for scout workflow.
const REQUIRED_SCOUT_PROMPTS: &[&str] = &["process", "synthesize", "qa", "research", "act"];

/// Allowed keys for `AgentConfig.per_state_limits`. These are the kebab-case
/// wire names of `ItemStatus` variants with a live CC/Codex session — the
/// only states where a per-instance concurrency cap makes sense. Mirrors
/// `captain::ItemStatus::is_active` (kept in sync by hand because the
/// `settings` crate does not depend on `captain`).
pub const ALLOWED_PER_STATE_LIMIT_KEYS: &[&str] = &[
    "in-progress",
    "clarifying",
    "captain-reviewing",
    "captain-merging",
];

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
    if agent.ops_timeout_s.is_zero() {
        errors.push("ops_timeout_s must be > 0".into());
    }
    if agent.codex_approval_policy != CodexApprovalPolicy::Never
        && agent.codex_approvals_reviewer == CodexApprovalsReviewer::User
    {
        errors.push(
            "codex_approvals_reviewer must be auto_review unless codex_approval_policy is never"
                .into(),
        );
    }
    if let Some(codex) = &agent.codex {
        if codex
            .model
            .as_deref()
            .is_some_and(|value| value.trim().is_empty())
        {
            errors.push("codex.model must not be empty when set".into());
        }
        if codex
            .service_tier
            .as_deref()
            .is_some_and(|value| value.trim().is_empty())
        {
            errors.push("codex.service_tier must not be empty when set".into());
        }
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

    for (key, value) in &agent.per_state_limits {
        if !ALLOWED_PER_STATE_LIMIT_KEYS.contains(&key.as_str()) {
            errors.push(format!(
                "per_state_limits: unknown state '{}' (allowed: {})",
                key,
                ALLOWED_PER_STATE_LIMIT_KEYS.join(", ")
            ));
        }
        if *value == 0 {
            errors.push(format!(
                "per_state_limits.{} must be > 0 (zero would block all dispatch in that state)",
                key
            ));
        }
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
        CaptainWorkflow::compiled_default().agent
    }

    #[test]
    fn default_agent_config_is_valid() {
        // Bundled YAML's tick_interval_s and stale_threshold_s satisfy the
        // stale_threshold_s >= 2 * tick_interval_s constraint.
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
    #[should_panic(expected = "codex_approvals_reviewer")]
    fn codex_on_request_requires_auto_review_panics() {
        let mut ac = default_agent();
        ac.codex_approval_policy = CodexApprovalPolicy::OnRequest;
        ac.codex_approvals_reviewer = CodexApprovalsReviewer::User;
        validate_agent_config(&ac, 30);
    }

    #[test]
    #[should_panic(expected = "codex.model")]
    fn empty_codex_model_panics() {
        let mut ac = default_agent();
        ac.codex = Some(super::super::workflow::CodexAgentConfig {
            model: Some(" ".into()),
            reasoning_effort: None,
            service_tier: Some("default".into()),
        });
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
    fn empty_per_state_limits_is_valid() {
        // Default behaviour is unchanged — empty map is the no-op state.
        let mut ac = default_agent();
        ac.per_state_limits.clear();
        validate_agent_config(&ac, 30);
    }

    #[test]
    fn known_per_state_keys_are_valid() {
        let mut ac = default_agent();
        for key in ALLOWED_PER_STATE_LIMIT_KEYS {
            ac.per_state_limits.insert((*key).into(), 1);
        }
        validate_agent_config(&ac, 30);
    }

    #[test]
    #[should_panic(expected = "per_state_limits: unknown state 'queued'")]
    fn unknown_per_state_key_rejected() {
        let mut ac = default_agent();
        ac.per_state_limits.insert("queued".into(), 1);
        validate_agent_config(&ac, 30);
    }

    #[test]
    #[should_panic(expected = "per_state_limits.in-progress must be > 0")]
    fn zero_per_state_limit_rejected() {
        let mut ac = default_agent();
        ac.per_state_limits.insert("in-progress".into(), 0);
        validate_agent_config(&ac, 30);
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
