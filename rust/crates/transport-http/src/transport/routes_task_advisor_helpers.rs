//! Helper functions for the task advisor route.

use rustc_hash::FxHashMap;

/// Build the advisor CC prompt by filling the workflow template with
/// task-level variables (title, status, project, PR, branch, context,
/// timeline, question, intent).
pub(crate) fn build_advisor_prompt(
    item: &captain::Task,
    task_id: &str,
    question: &str,
    intent: &str,
    workflow: &settings::CaptainWorkflow,
    timeline_text: &str,
) -> anyhow::Result<String> {
    render_advisor_template(
        "advisor",
        item,
        task_id,
        question,
        intent,
        workflow,
        timeline_text,
    )
}

/// Build the direct action-synthesis prompt used when no advisor session
/// exists yet. Renders `advisor_reopen_direct` with the full context vars.
pub(crate) fn build_advisor_action_prompt(
    item: &captain::Task,
    task_id: &str,
    question: &str,
    intent: &str,
    workflow: &settings::CaptainWorkflow,
    timeline_text: &str,
) -> anyhow::Result<String> {
    render_advisor_template(
        "advisor_reopen_direct",
        item,
        task_id,
        question,
        intent,
        workflow,
        timeline_text,
    )
}

/// Build the short synthesis directive used to resume an existing advisor
/// session. Renders `advisor_reopen_synthesis` with `intent`.
pub(crate) fn build_advisor_synthesis_prompt(
    intent: &str,
    workflow: &settings::CaptainWorkflow,
) -> anyhow::Result<String> {
    let mut vars: FxHashMap<&str, &str> = FxHashMap::default();
    vars.insert("intent", intent);
    settings::render_prompt("advisor_reopen_synthesis", &workflow.prompts, &vars)
        .map_err(|e| anyhow::anyhow!(e))
}

fn render_advisor_template(
    template_name: &str,
    item: &captain::Task,
    task_id: &str,
    question: &str,
    intent: &str,
    workflow: &settings::CaptainWorkflow,
    timeline_text: &str,
) -> anyhow::Result<String> {
    let mut vars: FxHashMap<&str, &str> = FxHashMap::default();
    vars.insert("title", &item.title);
    vars.insert("id", task_id);
    vars.insert("status", item.status().as_str());
    vars.insert("project", &item.project);
    let pr = item.pr_number.map(|n| n.to_string()).unwrap_or_default();
    vars.insert("pr", &pr);
    vars.insert("branch", item.branch.as_deref().unwrap_or(""));
    vars.insert("context", item.context.as_deref().unwrap_or(""));
    vars.insert("timeline", timeline_text);
    vars.insert("question", question);
    vars.insert("intent", intent);

    settings::render_prompt(template_name, &workflow.prompts, &vars).map_err(|e| anyhow::anyhow!(e))
}

/// Check if the given intent is allowed from the task's current status.
pub(crate) fn action_eligible(intent: &str, status: &captain::ItemStatus) -> bool {
    use captain::ItemStatus;
    match intent {
        "rework" => matches!(
            status,
            ItemStatus::AwaitingReview
                | ItemStatus::Escalated
                | ItemStatus::Errored
                | ItemStatus::HandedOff
        ),
        "revise-plan" => matches!(status, ItemStatus::PlanReady),
        _ => matches!(
            status,
            ItemStatus::AwaitingReview
                | ItemStatus::Escalated
                | ItemStatus::Errored
                | ItemStatus::HandedOff
                | ItemStatus::CompletedNoPr
                | ItemStatus::PlanReady
        ),
    }
}
