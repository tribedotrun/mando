use crate::config::{CaptainWorkflow, ScoutWorkflow};
use crate::types::WorkflowRuntimeMode;

pub fn apply_workflow_mode_overrides(
    mode: WorkflowRuntimeMode,
    captain_workflow: &mut CaptainWorkflow,
    scout_workflow: &mut ScoutWorkflow,
) {
    if let Some(model) = selected_model(mode) {
        apply_model_overrides(captain_workflow, scout_workflow, model);
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
        WorkflowRuntimeMode::Dev => Some("sonnet"),
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
