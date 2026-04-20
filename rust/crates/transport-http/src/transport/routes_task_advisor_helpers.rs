//! Helper functions for the task advisor route.

/// Build the advisor CC prompt by filling the workflow template with
/// task-level variables (title, status, project, PR, branch, context,
/// timeline, question).
pub(crate) fn build_advisor_prompt(
    item: &captain::Task,
    task_id: &str,
    question: &str,
    workflow: &settings::config::CaptainWorkflow,
    timeline_text: &str,
) -> anyhow::Result<String> {
    use rustc_hash::FxHashMap;

    let mut vars: FxHashMap<&str, &str> = FxHashMap::default();
    vars.insert("title", &item.title);
    vars.insert("id", task_id);
    let status_str = item.status().as_str();
    vars.insert("status", status_str);
    vars.insert("project", &item.project);
    let pr = item.pr_number.map(|n| n.to_string()).unwrap_or_default();
    vars.insert("pr", &pr);
    vars.insert("branch", item.branch.as_deref().unwrap_or(""));
    vars.insert("context", item.context.as_deref().unwrap_or(""));
    vars.insert("timeline", timeline_text);
    vars.insert("question", question);

    settings::config::render_prompt("advisor", &workflow.prompts, &vars)
        .map_err(|e| anyhow::anyhow!(e))
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
