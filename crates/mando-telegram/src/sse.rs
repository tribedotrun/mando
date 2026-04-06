//! SSE consumer for the gateway `/api/events` endpoint.
//!
//! Subscribes to the gateway's Server-Sent Events stream, parses incoming
//! events, and delivers them through a tokio mpsc channel. Auto-reconnects
//! on connection loss with exponential backoff.

use std::time::Duration;

use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::Value;
use tokio::sync::mpsc;

use mando_types::events::NotificationPayload;

/// Parsed SSE events from the gateway.
#[derive(Debug, Clone)]
pub enum SseEvent {
    /// Initial full-state snapshot (sent once on connect).
    Snapshot(Value),
    /// A notification payload (worker spawned, PR merged, etc.).
    Notification(NotificationPayload),
    /// Tasks changed.
    Tasks(Option<Value>),
    /// Status changed.
    Status(Option<Value>),
    /// Sessions changed.
    Sessions,
    /// Scout changed.
    Scout,
    /// Reconnected after a connection drop.
    Reconnected,
}

/// Consumer for the gateway SSE stream.
pub struct SseConsumer {
    base_url: String,
    token: Option<String>,
    client: reqwest::Client,
}

impl SseConsumer {
    /// Create a new SSE consumer.
    ///
    /// `base_url` — e.g. `http://127.0.0.1:18791`.
    /// `token` — the auth-token value (sent as Bearer header).
    pub fn new(base_url: &str, token: Option<String>) -> Self {
        Self {
            base_url: base_url.to_string(),
            token,
            client: reqwest::Client::new(),
        }
    }

    /// Start consuming SSE events. Returns a receiver that yields parsed events.
    ///
    /// Spawns a background task that:
    /// 1. Connects to `/api/events` with Bearer auth
    /// 2. Parses `data:` lines into `SseEvent` variants
    /// 3. Auto-reconnects on failure with exponential backoff (1s → 2s → 4s → … capped at 30s)
    /// 4. Sends `SseEvent::Reconnected` after each successful reconnect
    pub async fn subscribe(&self) -> Result<mpsc::Receiver<SseEvent>> {
        let (tx, rx) = mpsc::channel::<SseEvent>(256);

        let url = format!("{}/api/events", self.base_url);
        let client = self.client.clone();
        let token = self.token.clone();

        // TRACKED: the telegram bot (mando-tg) is a separate OS process from
        // the gateway and has no access to AppState. Its own shutdown signal
        // drops the mpsc receiver, which this task observes and exits cleanly.
        tokio::spawn(async move {
            let mut backoff = Duration::from_secs(1);
            let max_backoff = Duration::from_secs(30);

            loop {
                match connect_and_stream(&client, &url, token.as_deref(), &tx).await {
                    Ok(()) => {
                        // Stream ended cleanly — reset backoff since connection was healthy.
                        tracing::info!("SSE stream closed by server, reconnecting");
                        backoff = Duration::from_secs(1);
                    }
                    Err(e) => {
                        tracing::warn!("SSE connection error: {e:#}");
                        backoff = (backoff * 2).min(max_backoff);
                    }
                }

                // If the receiver was dropped, stop.
                if tx.is_closed() {
                    tracing::debug!("SSE receiver dropped, stopping consumer");
                    return;
                }

                tokio::time::sleep(backoff).await;

                // Signal reconnection to consumers.
                let _ = tx.send(SseEvent::Reconnected).await;

                tracing::info!("SSE reconnecting to {url}");
            }
        });

        Ok(rx)
    }
}

/// Connect to the SSE endpoint and stream events until disconnection.
async fn connect_and_stream(
    client: &reqwest::Client,
    url: &str,
    token: Option<&str>,
    tx: &mpsc::Sender<SseEvent>,
) -> Result<()> {
    let mut req = client.get(url).header("Accept", "text/event-stream");
    if let Some(t) = token {
        req = req.bearer_auth(t);
    }
    let mut resp = req.send().await.context("SSE connect failed")?;

    if !resp.status().is_success() {
        anyhow::bail!("SSE endpoint returned {}", resp.status());
    }

    let mut buf = String::new();

    // Use `chunk()` which doesn't require the `stream` reqwest feature.
    while let Some(chunk) = resp.chunk().await.context("SSE read error")? {
        let text = String::from_utf8_lossy(&chunk);
        buf.push_str(&text);

        // Process complete lines (SSE protocol: events separated by blank lines).
        while let Some(pos) = buf.find("\n\n") {
            let block = buf[..pos].to_string();
            buf.replace_range(..pos + 2, "");

            if let Some(event) = parse_sse_block(&block) {
                if tx.send(event).await.is_err() {
                    // Receiver dropped.
                    return Ok(());
                }
            }
        }
    }

    Ok(())
}

/// Parse an SSE block (one or more `data:` lines between blank-line delimiters).
fn parse_sse_block(block: &str) -> Option<SseEvent> {
    let mut data_parts = Vec::new();

    for line in block.lines() {
        if let Some(rest) = line.strip_prefix("data:") {
            data_parts.push(rest.trim());
        }
        // Ignore `event:`, `id:`, `retry:`, and comment lines (`:heartbeat`).
    }

    if data_parts.is_empty() {
        return None;
    }

    let data_str = data_parts.join("\n");

    // Try to parse as JSON.
    let json: Value = match serde_json::from_str(&data_str) {
        Ok(v) => v,
        Err(_) => {
            // Not JSON (e.g. heartbeat text). Skip.
            return None;
        }
    };

    parse_event_json(&json)
}

/// Wire format from the gateway: `{"event": "...", "ts": ..., "data": ...}`.
#[derive(Deserialize)]
struct WireEvent {
    event: String,
    data: Option<Value>,
}

fn parse_event_json(json: &Value) -> Option<SseEvent> {
    let wire: WireEvent = match serde_json::from_value(json.clone()) {
        Ok(w) => w,
        Err(e) => {
            tracing::warn!(module = "sse", error = %e, "failed to parse SSE wire event");
            return None;
        }
    };

    match wire.event.as_str() {
        "snapshot" => Some(SseEvent::Snapshot(wire.data.unwrap_or(Value::Null))),
        "notification" => {
            let data = wire.data?;
            let payload: NotificationPayload = match serde_json::from_value(data) {
                Ok(p) => p,
                Err(e) => {
                    tracing::warn!(module = "sse", error = %e, "failed to parse notification payload");
                    return None;
                }
            };
            Some(SseEvent::Notification(payload))
        }
        "tasks" => Some(SseEvent::Tasks(wire.data)),
        "status" => Some(SseEvent::Status(wire.data)),
        "sessions" => Some(SseEvent::Sessions),
        "scout" => Some(SseEvent::Scout),
        other => {
            tracing::debug!("unknown SSE event type: {other}");
            None
        }
    }
}

/// Run the SSE notification loop — subscribes to gateway events and forwards
/// notifications to Telegram. Reconnects automatically on failure.
///
/// Called from `main.rs` at startup (when owner is known) and from the bot
/// at runtime (when owner auto-registers via first message).
pub async fn run_notification_loop(
    base_url: String,
    token: Option<String>,
    api: crate::api::TelegramApi,
    chat_id: String,
) {
    let sse = SseConsumer::new(&base_url, token);
    let mut handler = crate::notifications::NotificationHandler::new(api, chat_id);

    let mut rx = match sse.subscribe().await {
        Ok(rx) => rx,
        Err(e) => {
            tracing::error!("SSE subscribe failed: {e}");
            return;
        }
    };

    while let Some(event) = rx.recv().await {
        match event {
            SseEvent::Notification(payload) => {
                handler.handle(payload).await;
            }
            SseEvent::Reconnected => {
                handler.clear_tracked_messages();
            }
            _ => {}
        }
    }
    tracing::warn!("SSE notification listener exited");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_snapshot_block() {
        let block = r#"data: {"event":"snapshot","ts":1234.5,"data":{"tasks":[]}}"#;
        let event = parse_sse_block(block).unwrap();
        assert!(matches!(event, SseEvent::Snapshot(_)));
    }

    #[test]
    fn parse_notification_block() {
        let block = r#"data: {"event":"notification","ts":1234.5,"data":{"message":"hello","level":"Normal","kind":{"type":"Generic"}}}"#;
        let event = parse_sse_block(block).unwrap();
        assert!(matches!(event, SseEvent::Notification(_)));
    }

    #[test]
    fn parse_tasks_block() {
        let block = r#"data: {"event":"tasks","ts":1234.5,"data":null}"#;
        let event = parse_sse_block(block).unwrap();
        assert!(matches!(event, SseEvent::Tasks(None)));
    }

    #[test]
    fn parse_heartbeat_ignored() {
        let block = ":heartbeat";
        let event = parse_sse_block(block);
        assert!(event.is_none());
    }

    #[test]
    fn parse_unknown_event() {
        let block = r#"data: {"event":"unknown_thing","ts":0,"data":null}"#;
        let event = parse_sse_block(block);
        assert!(event.is_none());
    }

    #[test]
    fn multi_line_data_non_json_ignored() {
        // SSE spec allows multi-line data: fields, but our gateway sends
        // single-line JSON. Multi-line data that isn't valid JSON is skipped.
        let block = "data: not json at all\ndata: more stuff";
        let event = parse_sse_block(block);
        assert!(event.is_none());
    }
}
