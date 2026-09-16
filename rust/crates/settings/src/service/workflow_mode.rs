use std::time::Duration;

use crate::config::settings::Config;
use crate::config::workflow::{AgentConfig, CodexAgentConfig};
use crate::config::{CaptainWorkflow, SandboxOverrides, ScoutWorkflow};
use crate::types::WorkflowRuntimeMode;

pub fn apply_workflow_mode_overrides(
    mode: WorkflowRuntimeMode,
    config: &mut Config,
    captain_workflow: &mut CaptainWorkflow,
    scout_workflow: &mut ScoutWorkflow,
) {
    if let Some(model) = selected_model(mode) {
        apply_model_overrides(captain_workflow, scout_workflow, model);
    }
    if matches!(mode, WorkflowRuntimeMode::Sandbox) {
        let overrides = captain_workflow.sandbox.clone();
        apply_sandbox_overrides(&overrides, config, &mut captain_workflow.agent);
        // The YAML passed validate_agent_config at load time against the
        // pre-override tick interval; re-validate now so a bad sandbox block
        // (e.g. stale_threshold_s < 2 * overridden tick_interval_s) surfaces
        // at daemon startup rather than on first tick.
        crate::config::workflow_validate::validate_agent_config(
            &captain_workflow.agent,
            config.captain.tick_interval_s,
        );
    }
}

pub fn apply_scout_workflow_mode_overrides(
    mode: WorkflowRuntimeMode,
    scout_workflow: &mut ScoutWorkflow,
) {
    if let Some(model) = selected_model(mode) {
        for scout_model in scout_workflow.models.values_mut() {
            *scout_model = model.into();
        }
    }
}

fn selected_model(mode: WorkflowRuntimeMode) -> Option<&'static str> {
    match mode {
        WorkflowRuntimeMode::Normal => None,
        WorkflowRuntimeMode::Dev => Some("haiku"),
        WorkflowRuntimeMode::Sandbox => Some("haiku"),
    }
}

fn apply_model_overrides(
    captain_workflow: &mut CaptainWorkflow,
    scout_workflow: &mut ScoutWorkflow,
    model: &str,
) {
    captain_workflow.models.worker = model.into();
    captain_workflow.models.captain = model.into();
    captain_workflow.models.clarifier = model.into();
    for scout_model in scout_workflow.models.values_mut() {
        *scout_model = model.into();
    }
}

fn apply_sandbox_overrides(
    overrides: &SandboxOverrides,
    config: &mut Config,
    agent: &mut AgentConfig,
) {
    if let Some(v) = overrides.tick_interval_s {
        config.captain.tick_interval_s = v;
    }
    if let Some(v) = overrides.stale_threshold_s {
        agent.stale_threshold_s = Duration::from_secs(v);
    }
    if let Some(v) = overrides.captain_review_timeout_s {
        agent.captain_review_timeout_s = Duration::from_secs(v);
    }
    if let Some(v) = overrides.captain_merge_timeout_s {
        agent.captain_merge_timeout_s = Duration::from_secs(v);
    }
    if let Some(v) = overrides.clarifier_timeout_s {
        agent.clarifier_timeout_s = Duration::from_secs(v);
    }
    if let Some(v) = overrides.worker_timeout_s {
        agent.worker_timeout_s = Duration::from_secs(v);
    }
    if let Some(v) = overrides.ops_timeout_s {
        agent.ops_timeout_s = Duration::from_secs(v);
    }
    if let Some(v) = overrides.no_pr_min_active_s {
        agent.no_pr_min_active_s = Duration::from_secs(v);
    }
    if let Some(v) = overrides.max_interventions {
        agent.max_interventions = v;
    }
    if let Some(codex) = &overrides.codex {
        merge_codex_config(&mut agent.codex, codex);
    }
}

fn merge_codex_config(target: &mut Option<CodexAgentConfig>, overrides: &CodexAgentConfig) {
    let mut merged = target.clone().unwrap_or(CodexAgentConfig {
        model: None,
        reasoning_effort: None,
        service_tier: None,
    });
    if overrides.model.is_some() {
        merged.model = overrides.model.clone();
    }
    if overrides.reasoning_effort.is_some() {
        merged.reasoning_effort = overrides.reasoning_effort;
    }
    if overrides.service_tier.is_some() {
        merged.service_tier = overrides.service_tier.clone();
    }
    *target = Some(merged);
}
