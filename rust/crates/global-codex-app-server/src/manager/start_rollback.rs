use serde_json::json;

use super::CodexAppServerManager;

pub(super) async fn rollback_started_thread(
    manager: &CodexAppServerManager,
    thread_id: &str,
    turn_id: Option<&str>,
    response_timeout: std::time::Duration,
    reason: &'static str,
) {
    manager.inner.active_turns.lock().await.remove(thread_id);
    manager.unsubscribe_local(thread_id).await;
    if manager.process_info().await.is_none() {
        return;
    }
    if let Some(turn_id) = turn_id {
        if let Err(e) = manager
            .request(
                "turn/interrupt",
                json!({"threadId": thread_id, "turnId": turn_id}),
                response_timeout,
            )
            .await
        {
            tracing::warn!(
                module = "codex_app_server",
                thread_id,
                turn_id,
                reason,
                error = %e,
                "failed to interrupt Codex turn during start rollback"
            );
        }
    }
    if let Err(e) = manager
        .request(
            "thread/unsubscribe",
            json!({"threadId": thread_id}),
            response_timeout,
        )
        .await
    {
        tracing::warn!(
            module = "codex_app_server",
            thread_id,
            reason,
            error = %e,
            "failed to unsubscribe Codex thread during start rollback"
        );
    }
}
