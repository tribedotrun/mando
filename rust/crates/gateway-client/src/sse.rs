//! Typed SSE consumer for the gateway `/api/events` endpoint.
//!
//! The decode path is a single `serde_json::from_str::<SseEnvelope>(...)` —
//! no `Value` hop, no hand-rolled mirrors. Envelope variants drop at this
//! boundary when unknown, courtesy of `deny_unknown_fields` on every
//! api_types struct.

use std::time::Duration;

use anyhow::{Context, Result};
use serde::Deserialize;
use thiserror::Error;
use tokio::sync::mpsc;

use api_types::SseEnvelope;

/// Every `event` tag SseEnvelope currently carries. Gateway emits only
/// these; a future variant shows up here before the strict decode runs.
const KNOWN_EVENT_TAGS: &[&str] = &[
    "snapshot",
    "snapshot_error",
    "resync",
    "tasks",
    "scout",
    "status",
    "sessions",
    "notification",
    "workbenches",
    "config",
    "research",
    "credentials",
    "artifacts",
];

#[derive(Debug, Deserialize)]
struct EnvelopeTagProbe<'a> {
    event: &'a str,
}

/// Parse failures from a single SSE `data:` block.
#[derive(Debug, Error)]
pub enum ParseError {
    /// Block held `data:` lines that did not concatenate into valid JSON.
    #[error("SSE block is not valid JSON")]
    InvalidJson(#[from] serde_json::Error),
}

/// Parse one SSE block.
///
/// - `Ok(Some(env))` — a valid SSE envelope worth forwarding.
/// - `Ok(None)` — heartbeat, unknown event, or empty block.
/// - `Err(e)` — the block held JSON-shaped data that failed to decode.
pub fn parse_sse_block(block: &str) -> Result<Option<SseEnvelope>, ParseError> {
    let mut data_parts: Vec<&str> = Vec::new();
    for line in block.lines() {
        if let Some(rest) = line.strip_prefix("data:") {
            data_parts.push(rest.trim());
        }
        // `event:` / `id:` / `retry:` / comment lines (`:heartbeat`) ignored.
    }
    if data_parts.is_empty() {
        return Ok(None);
    }

    let data_str = data_parts.join("\n");

    // Heartbeats and other non-JSON payloads are silently ignored so the
    // wire stays noise-tolerant — but any structurally-JSON payload that
    // fails to decode is surfaced as an error the caller can log.
    let first = data_str.trim_start().chars().next();
    let looks_like_json = matches!(first, Some('{' | '['));
    if !looks_like_json {
        return Ok(None);
    }

    // Pre-check the `event` tag against the known set so unknown variants
    // drop silently (matches the design intent that new variants roll out
    // producer-first). Typed-probe deserialize — not substring matching.
    if let Ok(probe) = serde_json::from_str::<EnvelopeTagProbe>(&data_str) {
        if !KNOWN_EVENT_TAGS.contains(&probe.event) {
            return Ok(None);
        }
    }

    match serde_json::from_str::<SseEnvelope>(&data_str) {
        Ok(env) => Ok(Some(env)),
        Err(e) => Err(ParseError::InvalidJson(e)),
    }
}

/// End-to-end SSE consumer: subscribe + reconnect loop.
pub struct SseConsumer {
    base_url: String,
    token: Option<String>,
    client: reqwest::Client,
}

impl SseConsumer {
    pub fn new(base_url: &str, token: Option<String>) -> Self {
        Self {
            base_url: base_url.to_string(),
            token,
            client: (*global_net::sse_client()).clone(),
        }
    }

    /// Subscribe to the gateway SSE stream.
    ///
    /// Yields typed `api_types::SseEnvelope`. `None`-returning blocks
    /// (heartbeats, unknown variants) are filtered out at this layer so
    /// callers see only valid events. Reconnection is signaled by a
    /// channel-level `SseEvent::Reconnected` style — handled by forwarding
    /// an extra [`SseSignal::Reconnected`] through the receiver.
    pub async fn subscribe(&self) -> Result<mpsc::Receiver<SseSignal>> {
        let (tx, rx) = mpsc::channel::<SseSignal>(256);

        let url = format!("{}/api/events", self.base_url);
        let client = self.client.clone();
        let token = self.token.clone();

        tokio::spawn(async move {
            let mut backoff = Duration::from_secs(1);
            let max_backoff = Duration::from_secs(30);
            let mut connected_once = false;
            let mut logged_initial_wait = false;

            loop {
                match connect_and_stream(&client, &url, token.as_deref(), &tx).await {
                    Ok(()) => {
                        tracing::info!(
                            module = "gateway-client-sse",
                            "SSE stream closed by server, reconnecting"
                        );
                        backoff = Duration::from_secs(1);
                        connected_once = true;
                    }
                    Err(e) => {
                        if connected_once {
                            tracing::warn!(
                                module = "gateway-client-sse",
                                "SSE connection error: {e:#}"
                            );
                        } else if !logged_initial_wait {
                            tracing::info!(
                                module = "gateway-client-sse",
                                "SSE waiting for gateway stream: {e:#}"
                            );
                            logged_initial_wait = true;
                        } else {
                            tracing::debug!("SSE waiting for gateway stream: {e:#}");
                        }
                        backoff = (backoff * 2).min(max_backoff);
                    }
                }

                if tx.is_closed() {
                    tracing::debug!("SSE receiver dropped, stopping consumer");
                    return;
                }

                tokio::time::sleep(backoff).await;
                let _send_result = tx.send(SseSignal::Reconnected).await;
                tracing::info!(module = "gateway-client-sse", "SSE reconnecting to {url}");
            }
        });

        Ok(rx)
    }
}

/// Signal forwarded through the SSE subscribe channel.
#[derive(Debug, Clone)]
pub enum SseSignal {
    /// A decoded typed envelope.
    Envelope(Box<SseEnvelope>),
    /// Reconnection signal (caller may want to clear stale local state).
    Reconnected,
}

async fn connect_and_stream(
    client: &reqwest::Client,
    url: &str,
    token: Option<&str>,
    tx: &mpsc::Sender<SseSignal>,
) -> Result<()> {
    let mut req = client.get(url).header("Accept", "text/event-stream");
    if let Some(t) = token {
        req = req.bearer_auth(t);
    }
    let mut resp = req.send().await.context("SSE connect failed")?;

    if !resp.status().is_success() {
        anyhow::bail!("SSE endpoint returned {}", resp.status());
    }

    let mut buf: Vec<u8> = Vec::new();

    while let Some(chunk) = resp.chunk().await.context("SSE read error")? {
        buf.extend_from_slice(&chunk);

        while let Some(block) = drain_sse_block(&mut buf) {
            match parse_sse_block(&block) {
                Ok(Some(env)) => {
                    if tx.send(SseSignal::Envelope(Box::new(env))).await.is_err() {
                        return Ok(());
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    tracing::error!(
                        module = "gateway-client-sse",
                        error = %e,
                        "failed to decode SSE block"
                    );
                }
            }
        }
    }

    Ok(())
}

/// Drain the next `\n\n`-terminated block from a raw-byte accumulator.
///
/// UTF-8 decoding runs only on complete blocks, so multi-byte codepoints
/// that straddle a `resp.chunk()` boundary are never fragmented into
/// replacement chars. `\n` (0x0A) is never a UTF-8 continuation byte, so
/// the bytes before the terminator are always a full sequence of
/// codepoints even when the server flushes mid-stream.
fn drain_sse_block(buf: &mut Vec<u8>) -> Option<String> {
    let pos = buf.windows(2).position(|w| w == b"\n\n")?;
    let bytes: Vec<u8> = buf.drain(..pos + 2).collect();
    Some(String::from_utf8_lossy(&bytes[..pos]).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use api_types::{NotificationEventPayload, NotificationKind, NotificationPayload, NotifyLevel};

    fn notification_block(message: &str) -> String {
        let env = SseEnvelope::Notification(NotificationEventPayload {
            ts: 1.0,
            data: Some(NotificationPayload {
                message: message.to_string(),
                level: NotifyLevel::Normal,
                kind: NotificationKind::Generic,
                task_key: None,
                reply_markup: None,
            }),
        });
        format!("data: {}", serde_json::to_string(&env).unwrap())
    }

    #[test]
    fn parses_valid_envelope() {
        let env = parse_sse_block(&notification_block("hi"))
            .expect("block parses")
            .expect("envelope present");
        match env {
            SseEnvelope::Notification(p) => {
                let data = p.data.unwrap();
                assert_eq!(data.message, "hi");
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn heartbeat_returns_none() {
        let r = parse_sse_block(":heartbeat").unwrap();
        assert!(r.is_none());
    }

    #[test]
    fn unknown_variant_returns_none() {
        let block = r#"data: {"event":"no_such_event","ts":0,"data":null}"#;
        let r = parse_sse_block(block).unwrap();
        assert!(r.is_none());
    }

    #[test]
    fn empty_block_returns_none() {
        let r = parse_sse_block("").unwrap();
        assert!(r.is_none());
    }

    #[test]
    fn multiline_non_json_returns_none() {
        let block = "data: not json at all\ndata: more stuff";
        let r = parse_sse_block(block).unwrap();
        assert!(r.is_none());
    }

    #[test]
    fn malformed_json_returns_error() {
        let block = r#"data: {"event":"notification", broken"#;
        assert!(parse_sse_block(block).is_err());
    }

    #[test]
    fn drain_sse_block_reassembles_multibyte_utf8_across_chunks() {
        // Regression: `String::from_utf8_lossy` per-chunk used to replace
        // multi-byte codepoints straddling a `resp.chunk()` boundary with
        // U+FFFD, silently dropping SSE events whose JSON string content
        // held non-ASCII text (CJK, RTL scripts, emoji).
        let frame = format!("{}\n\n", notification_block("中文 مرحبا 👋"));
        let bytes = frame.as_bytes();

        let mut buf: Vec<u8> = Vec::new();
        let mut assembled: Option<String> = None;
        for byte in bytes {
            buf.push(*byte);
            if let Some(block) = drain_sse_block(&mut buf) {
                assembled = Some(block);
            }
        }

        let block = assembled.expect("full block drained");
        let env = parse_sse_block(&block)
            .expect("block parses")
            .expect("envelope present");
        match env {
            SseEnvelope::Notification(p) => {
                assert_eq!(p.data.unwrap().message, "中文 مرحبا 👋");
            }
            other => panic!("unexpected {other:?}"),
        }
        assert!(buf.is_empty(), "nothing should remain after full drain");
    }

    #[test]
    fn parse_notification_rejects_flat_envelope() {
        // Pre-#842 shape. deny_unknown_fields on NotificationEventPayload
        // surfaces this as a JSON decode error (not an unknown variant),
        // which we propagate as Err — caller logs loudly.
        let block = r#"data: {"event":"notification","ts":1.0,"data":{"message":"hi","level":"Normal","kind":{"type":"Generic"}}}"#;
        assert!(parse_sse_block(block).is_err());
    }
}
