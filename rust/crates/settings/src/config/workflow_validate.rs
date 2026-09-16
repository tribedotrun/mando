//! Startup validation for workflow YAML templates.
//!
//! Typed deserialization guarantees required workflow keys; this module checks
//! template syntax and semantic constraints at gateway startup.

use super::workflow::{
    AgentConfig, CaptainWorkflow, CodexApprovalPolicy, CodexApprovalsReviewer, ScoutWorkflow,
    StageAgentConfig,
};
use global_claude::CcStreamSymptom;

/// Every `CcStreamSymptom` variant the compiled binary routes on. A user
/// workflow override must declare a rule for each variant — missing a variant
/// would silently disable broken-session detection for that failure mode.
/// Keep this list in sync with the enum.
const REQUIRED_STREAM_SYMPTOMS: &[CcStreamSymptom] = &[
    CcStreamSymptom::StreamIdleTimeout,
    CcStreamSymptom::RateLimitAborted,
    CcStreamSymptom::IsError,
    CcStreamSymptom::ContextLengthExceeded,
    CcStreamSymptom::NoConversationFound,
    CcStreamSymptom::SessionInterrupted,
];

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

/// Validate captain template syntax and semantic workflow rules.
/// Required and unknown keys are enforced while deserializing the typed shape.
/// Panics on any errors — call at startup to fail fast.
pub fn validate_captain_workflow(wf: &CaptainWorkflow) {
    let mut errors = Vec::new();
    collect_template_errors("prompts", &wf.prompts, &mut errors);
    collect_template_errors("nudges", &wf.nudges, &mut errors);
    collect_template_errors("initial_prompts", &wf.initial_prompts, &mut errors);
    validate_stream_symptoms(&wf.stream_symptoms, &mut errors);
    validate_stage_agent(
        "stages.implementation",
        &wf.stages.implementation,
        &mut errors,
    );
    if !errors.is_empty() {
        global_infra::unrecoverable!(format!(
            "captain workflow validation failed: {}",
            errors.join(", ")
        ));
    }
}

fn validate_stage_agent(scope: &str, config: &StageAgentConfig, errors: &mut Vec<String>) {
    if config.model.trim().is_empty() {
        errors.push(format!("{scope}.model must not be empty"));
    }
    if config.session_start_timeout_s.is_zero() {
        errors.push(format!("{scope}.session_start_timeout_s must be > 0"));
    }
    if config.adapter == api_types::TaskProvider::OpenCode
        && config.variant.as_deref() != Some("max")
    {
        errors.push(format!(
            "{scope}.variant must be max for OpenCode GLM routing"
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
        if let Err(error) = super::workflow_typed::validate_per_state_limit_key(key) {
            errors.push(error);
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
