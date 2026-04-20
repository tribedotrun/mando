use crate::types::AppState;

#[tracing::instrument(skip_all)]
pub async fn close_ask_session(state: &AppState, task_id: i64) {
    let key = format!("task-ask:{task_id}");
    state.sessions.close_async(&key).await;
}

#[allow(dead_code)]
#[tracing::instrument(skip_all)]
pub async fn clear_advisor_session(state: &AppState, task_id: i64) {
    let key = format!("advisor:{task_id}");
    state.sessions.close_async(&key).await;
}
