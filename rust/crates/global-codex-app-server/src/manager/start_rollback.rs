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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn rollback_clears_local_state_when_process_is_unavailable() {
        let manager = CodexAppServerManager::new();
        let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel();
        manager
            .inner
            .subscribers
            .lock()
            .await
            .insert("thread".into(), event_tx);
        manager.inner.active_turns.lock().await.insert(
            "thread".into(),
            super::super::ActiveTurn {
                turn_id: "turn".into(),
                response_timeout: std::time::Duration::from_millis(1),
            },
        );

        rollback_started_thread(
            &manager,
            "thread",
            Some("turn"),
            std::time::Duration::from_millis(1),
            "test",
        )
        .await;

        assert!(!manager
            .inner
            .subscribers
            .lock()
            .await
            .contains_key("thread"));
        assert!(!manager
            .inner
            .active_turns
            .lock()
            .await
            .contains_key("thread"));
        assert!(matches!(
            event_rx.try_recv(),
            Err(tokio::sync::mpsc::error::TryRecvError::Disconnected)
        ));
    }
}
