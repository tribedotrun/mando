//! Telegram-bot SSE adapter — thin wrapper around `gateway-client::SseConsumer`.
//!
//! The underlying consumer returns `api_types::SseEnvelope` directly. This
//! module pattern-matches the envelope variants and routes them to the
//! existing `NotificationHandler`. No `serde_json::Value` hop, no
//! hand-rolled wire mirror, no `from_value::<T>` escape.

use api_types::SseEnvelope;
use gateway_client::{SseConsumer, SseSignal};

pub async fn run_notification_loop(
    base_url: String,
    token: Option<String>,
    api: crate::api::TelegramApi,
    chat_id: String,
    gw: crate::http::GatewayClient,
    pending: crate::PendingMessages,
) {
    let sse = SseConsumer::new(&base_url, token);
    let mut handler = crate::notifications::NotificationHandler::new(api, chat_id, gw, pending);

    let mut rx = match sse.subscribe().await {
        Ok(rx) => rx,
        Err(e) => {
            tracing::error!(module = "transport-tg-sse", "SSE subscribe failed: {e}");
            return;
        }
    };

    while let Some(signal) = rx.recv().await {
        match signal {
            SseSignal::Envelope(env) => dispatch(&mut handler, *env).await,
            SseSignal::Reconnected => handler.clear_tracked_messages(),
        }
    }
    tracing::warn!(
        module = "transport-tg-sse",
        "SSE notification listener exited"
    );
}

async fn dispatch(handler: &mut crate::notifications::NotificationHandler, env: SseEnvelope) {
    match env {
        SseEnvelope::Notification(payload) => {
            if let Some(data) = payload.data {
                handler.handle(data).await;
            } else {
                tracing::error!(
                    module = "transport-tg-sse",
                    ts = payload.ts,
                    "notification envelope arrived with null payload"
                );
            }
        }
        SseEnvelope::Research(payload) => {
            if let Some(data) = payload.data {
                handler.handle_research(data).await;
            }
        }
        // ^^ `data` is now `api_types::ResearchEventData` (typed); the
        // handler decodes fields through accessors, not `Value` indexing.
        // Other variants are not handled by the telegram bot today —
        // tasks/status/scout/etc flow to the Electron renderer, not TG.
        SseEnvelope::Snapshot(_)
        | SseEnvelope::SnapshotError(_)
        | SseEnvelope::Resync(_)
        | SseEnvelope::Tasks(_)
        | SseEnvelope::Scout(_)
        | SseEnvelope::Status(_)
        | SseEnvelope::Sessions(_)
        | SseEnvelope::Workbenches(_)
        | SseEnvelope::Config(_)
        | SseEnvelope::Credentials(_)
        | SseEnvelope::Artifacts(_) => {}
    }
}
