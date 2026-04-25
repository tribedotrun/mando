use std::time::Duration;

use crate::config::settings::Config;
use crate::config::workflow::AgentConfig;
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
        apply_sandbox_timing_overrides(&overrides, config, &mut captain_workflow.agent);
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
    captain_workflow.models.todo_parse = model.into();
    for scout_model in scout_workflow.models.values_mut() {
        *scout_model = model.into();
    }
}

fn apply_sandbox_timing_overrides(
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
    if let Some(v) = overrides.task_ask_timeout_s {
        agent.task_ask_timeout_s = Duration::from_secs(v);
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_captain_with_sandbox_block() -> CaptainWorkflow {
        CaptainWorkflow {
            sandbox: SandboxOverrides {
                tick_interval_s: Some(2),
                stale_threshold_s: Some(60),
                captain_review_timeout_s: Some(120),
                captain_merge_timeout_s: Some(180),
                clarifier_timeout_s: Some(120),
                worker_timeout_s: Some(600),
                task_ask_timeout_s: Some(60),
                ops_timeout_s: Some(30),
                no_pr_min_active_s: Some(0),
                max_interventions: Some(1),
            },
            ..Default::default()
        }
    }

    #[test]
    fn sandbox_mode_applies_timing_overrides() {
        let mut config = Config::default();
        config.captain.tick_interval_s = 30;
        let mut captain = default_captain_with_sandbox_block();
        let mut scout = ScoutWorkflow::default();

        apply_workflow_mode_overrides(
            WorkflowRuntimeMode::Sandbox,
            &mut config,
            &mut captain,
            &mut scout,
        );

        assert_eq!(config.captain.tick_interval_s, 2);
        assert_eq!(captain.agent.stale_threshold_s, Duration::from_secs(60));
        assert_eq!(
            captain.agent.captain_review_timeout_s,
            Duration::from_secs(120)
        );
        assert_eq!(
            captain.agent.captain_merge_timeout_s,
            Duration::from_secs(180)
        );
        assert_eq!(captain.agent.clarifier_timeout_s, Duration::from_secs(120));
        assert_eq!(captain.agent.worker_timeout_s, Duration::from_secs(600));
        assert_eq!(captain.agent.task_ask_timeout_s, Duration::from_secs(60));
        assert_eq!(captain.agent.ops_timeout_s, Duration::from_secs(30));
        assert_eq!(captain.agent.no_pr_min_active_s, Duration::from_secs(0));
        assert_eq!(captain.models.worker, "haiku");
    }

    #[test]
    fn sandbox_mode_skips_absent_overrides() {
        let mut config = Config::default();
        let original_tick = config.captain.tick_interval_s;
        let mut captain = CaptainWorkflow::default();
        let original_stale = captain.agent.stale_threshold_s;
        let original_worker_timeout = captain.agent.worker_timeout_s;
        let mut scout = ScoutWorkflow::default();

        apply_workflow_mode_overrides(
            WorkflowRuntimeMode::Sandbox,
            &mut config,
            &mut captain,
            &mut scout,
        );

        assert_eq!(config.captain.tick_interval_s, original_tick);
        assert_eq!(captain.agent.stale_threshold_s, original_stale);
        assert_eq!(captain.agent.worker_timeout_s, original_worker_timeout);
        assert_eq!(captain.models.worker, "haiku");
    }

    #[test]
    fn normal_mode_never_applies_sandbox_overrides() {
        let mut config = Config::default();
        config.captain.tick_interval_s = 30;
        let mut captain = default_captain_with_sandbox_block();
        let mut scout = ScoutWorkflow::default();

        apply_workflow_mode_overrides(
            WorkflowRuntimeMode::Normal,
            &mut config,
            &mut captain,
            &mut scout,
        );

        assert_eq!(config.captain.tick_interval_s, 30);
        assert_eq!(
            captain.agent.stale_threshold_s,
            AgentConfig::default().stale_threshold_s
        );
        assert_eq!(captain.models.worker, "default");
    }

    #[test]
    fn dev_mode_applies_model_but_not_timing() {
        let mut config = Config::default();
        let original_tick = config.captain.tick_interval_s;
        let mut captain = default_captain_with_sandbox_block();
        let mut scout = ScoutWorkflow::default();

        apply_workflow_mode_overrides(
            WorkflowRuntimeMode::Dev,
            &mut config,
            &mut captain,
            &mut scout,
        );

        assert_eq!(config.captain.tick_interval_s, original_tick);
        assert_eq!(
            captain.agent.stale_threshold_s,
            AgentConfig::default().stale_threshold_s
        );
        assert_eq!(captain.models.worker, "haiku");
    }
}
