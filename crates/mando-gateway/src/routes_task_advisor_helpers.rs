//! Helper functions for the task advisor route.

use crate::AppState;

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

/// Clear session_ids.advisor on a task.
pub(crate) async fn clear_advisor_session(state: &AppState, task_id: i64) {
    let store = state.task_store.write().await;
    match store.find_by_id(task_id).await {
        Ok(Some(mut task)) if task.session_ids.advisor.is_some() => {
            task.session_ids.advisor = None;
            if let Err(e) = store.write_task(&task).await {
                tracing::warn!(task_id, error = %e, "failed to clear session_ids.advisor");
            }
        }
        Ok(_) => {}
        Err(e) => {
            tracing::warn!(task_id, error = %e, "failed to read task for advisor session clear")
        }
    }
}
