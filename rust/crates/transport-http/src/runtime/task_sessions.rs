use crate::types::AppState;

#[tracing::instrument(skip_all)]
pub async fn close_ask_session(state: &AppState, task_id: i64) {
    let key = format!("task-ask:{task_id}");
    state.sessions.close_async(&key).await;
}

#[tracing::instrument(skip_all)]
pub async fn close_advisor_session(state: &AppState, task_id: i64) {
    let key = format!("advisor:{task_id}");
    state.sessions.close_async(&key).await;
}

#[tracing::instrument(skip_all)]
pub async fn clear_advisor_session(state: &AppState, task_id: i64) {
    close_advisor_session(state, task_id).await;
    if let Err(e) = state.captain.set_task_advisor_session(task_id, None).await {
        tracing::warn!(module = "transport-http-runtime-task_sessions", task_id, error = %e, "failed to clear session_ids.advisor");
    }
}
