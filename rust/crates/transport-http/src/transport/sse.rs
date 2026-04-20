//! /api/events — Server-Sent Events endpoint.

use std::convert::Infallible;
use std::time::Duration;

use anyhow::Context;
use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures_util::stream::Stream;
use serde::de::DeserializeOwned;
use serde::Serialize;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;

use crate::AppState;

/// GET /api/events — SSE stream of bus events.
///
/// Sends an initial `snapshot` event with full state, then streams
/// incremental deltas from the event bus.
pub(crate) async fn sse_events(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    // Build initial snapshot from current state. On failure, send an explicit
    // error frame so clients reload via the REST APIs instead of seeing
    // empty-state silently.
    let snapshot_event = match build_snapshot(&state).await {
        Ok(data) => {
            let envelope =
                api_types::SseEnvelope::Snapshot(api_types::SnapshotPayload { ts: now_ts(), data });
            encode_envelope(&envelope)
        }
        Err(e) => {
            tracing::error!(module = "transport-http-transport-sse", error = %e, "SSE snapshot: failed to build snapshot");
            let envelope = api_types::SseEnvelope::SnapshotError(api_types::SnapshotErrorPayload {
                ts: now_ts(),
                data: api_types::SseSnapshotErrorData {
                    message: format!("snapshot build failed: {e}"),
                    retry: true,
                },
            });
            encode_envelope(&envelope)
        }
    };
    let snapshot_stream = futures_util::stream::once(async { Ok(snapshot_event) });

    // Live event stream (incremental deltas).
    let rx = state.bus.subscribe();
    let stream = BroadcastStream::new(rx);

    let event_stream = stream.filter_map(|result| {
        match result {
            Ok(payload) => {
                let envelope = bus_payload_to_envelope(payload, now_ts());
                Some(Ok(encode_envelope(&envelope)))
            }
            Err(lag_err) => {
                // Slow consumer: the broadcast channel dropped events.
                // Emit a typed `resync` envelope so the client reloads full
                // state via the REST APIs instead of silently missing
                // messages.
                let skipped = format!("{lag_err}");
                tracing::warn!(
                    module = "sse",
                    error = %skipped,
                    "SSE client lagged, broadcast events dropped"
                );
                let envelope = api_types::SseEnvelope::Resync(api_types::ResyncPayload {
                    ts: now_ts(),
                    data: api_types::SseResyncData {
                        reason: skipped,
                        reload: vec![
                            "/api/tasks".into(),
                            "/api/scout".into(),
                            "/api/sessions".into(),
                            "/api/workers".into(),
                            "/api/workbenches".into(),
                            "/api/config".into(),
                            "/api/credentials".into(),
                        ],
                    },
                });
                Some(Ok(encode_envelope(&envelope)))
            }
        }
    });

    // Prepend snapshot, then live events.
    let combined = snapshot_stream.chain(event_stream);

    Sse::new(combined).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("heartbeat"),
    )
}

/// Serialize an SSE envelope into an `Event`. Serialization only fails on
/// arithmetic overflow of floats or invalid Unicode, neither of which
/// our typed envelopes can produce; on the unreachable error path emit
/// an empty event so the stream continues rather than panicking.
fn encode_envelope(envelope: &api_types::SseEnvelope) -> Event {
    match serde_json::to_string(envelope) {
        Ok(s) => Event::default().data(s),
        Err(e) => {
            tracing::error!(target: "transport-http-sse", module = "transport-http", %e, "failed to encode SSE envelope");
            Event::default().data("{}")
        }
    }
}

fn bus_payload_to_envelope(payload: global_bus::BusPayload, ts: f64) -> api_types::SseEnvelope {
    match payload {
        global_bus::BusPayload::Tasks(data) => {
            api_types::SseEnvelope::Tasks(api_types::TasksPayload { ts, data })
        }
        global_bus::BusPayload::Scout(data) => {
            api_types::SseEnvelope::Scout(api_types::ScoutPayload { ts, data })
        }
        global_bus::BusPayload::Status(data) => {
            api_types::SseEnvelope::Status(api_types::StatusPayload { ts, data })
        }
        global_bus::BusPayload::Sessions(data) => {
            api_types::SseEnvelope::Sessions(api_types::SessionsPayload { ts, data })
        }
        global_bus::BusPayload::Notification(data) => {
            api_types::SseEnvelope::Notification(api_types::NotificationEventPayload {
                ts,
                data: Some(data),
            })
        }
        global_bus::BusPayload::Workbenches(data) => {
            api_types::SseEnvelope::Workbenches(api_types::WorkbenchesPayload { ts, data })
        }
        global_bus::BusPayload::Config(data) => {
            api_types::SseEnvelope::Config(api_types::ConfigPayload {
                ts,
                data: data.map(|b| *b),
            })
        }
        global_bus::BusPayload::Research(data) => {
            api_types::SseEnvelope::Research(api_types::ResearchPayload { ts, data })
        }
        global_bus::BusPayload::Credentials(data) => {
            api_types::SseEnvelope::Credentials(api_types::CredentialsPayload { ts, data })
        }
        global_bus::BusPayload::Artifacts(data) => {
            api_types::SseEnvelope::Artifacts(api_types::ArtifactsPayload { ts, data })
        }
    }
}

fn now_ts() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs_f64()
}

/// Build a full state snapshot for the initial SSE event.
async fn build_snapshot(state: &AppState) -> anyhow::Result<api_types::SseSnapshotData> {
    let (all_items, worker_rows, workbench_rows) = state.captain.load_sse_snapshot_data().await?;
    let tasks = roundtrip::<Vec<api_types::TaskItem>>(&all_items, "tasks")?;
    let workers = roundtrip::<Vec<api_types::WorkerDetail>>(worker_rows, "workers")?;
    let workbenches = roundtrip::<Vec<api_types::WorkbenchItem>>(workbench_rows, "workbenches")?;
    let terminals =
        roundtrip::<Vec<api_types::TerminalSessionInfo>>(state.terminal.list(), "terminals")?;

    let config = {
        let cfg = state.settings.load_config();
        let mut value = serde_json::to_value(&*cfg).context("failed to serialize config")?;
        crate::runtime::config_support::inject_projects(&cfg, &mut value);
        roundtrip::<api_types::MandoConfig>(value, "config")?
    };

    let uptime = state.start_time.elapsed().as_secs();

    Ok(api_types::SseSnapshotData {
        tasks,
        workers,
        workbenches,
        terminals,
        config,
        daemon: api_types::SseDaemonInfo {
            version: env!("CARGO_PKG_VERSION").to_string(),
            uptime,
        },
    })
}

#[cfg(test)]
pub(crate) fn resync_envelope(ts: f64, reason: String) -> api_types::SseEnvelope {
    api_types::SseEnvelope::Resync(api_types::ResyncPayload {
        ts,
        data: api_types::SseResyncData {
            reason,
            reload: vec![
                "/api/tasks".into(),
                "/api/scout".into(),
                "/api/sessions".into(),
                "/api/workers".into(),
                "/api/workbenches".into(),
                "/api/config".into(),
                "/api/credentials".into(),
            ],
        },
    })
}

fn roundtrip<T: DeserializeOwned>(value: impl Serialize, label: &'static str) -> anyhow::Result<T> {
    let json =
        serde_json::to_value(value).with_context(|| format!("failed to serialize {label}"))?;
    serde_json::from_value(json)
        .with_context(|| format!("failed to deserialize {label} into api-types"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use global_bus::BusPayload;

    #[tokio::test]
    async fn sse_receives_event() {
        let bus = global_bus::EventBus::new();
        let mut rx = bus.subscribe();

        bus.send(BusPayload::Tasks(Some(api_types::TaskEventData {
            action: Some("created".into()),
            item: None,
            id: Some(1),
            cleared_by: None,
        })));

        let payload = rx.recv().await.unwrap();
        assert!(matches!(payload, BusPayload::Tasks(Some(_))));
    }

    #[test]
    fn typed_payload_produces_tasks_envelope() {
        let payload = BusPayload::Tasks(None);
        let envelope = bus_payload_to_envelope(payload, 12.0);
        match envelope {
            api_types::SseEnvelope::Tasks(p) => {
                assert_eq!(p.ts, 12.0);
                assert!(p.data.is_none());
            }
            other => panic!("expected Tasks envelope, got {other:?}"),
        }
    }

    #[test]
    fn broadcast_lag_produces_resync_envelope() {
        let envelope = super::resync_envelope(5.0, "lagged 3 messages".into());
        match envelope {
            api_types::SseEnvelope::Resync(payload) => {
                assert!(payload.data.reason.contains("lagged"));
                assert!(payload
                    .data
                    .reload
                    .iter()
                    .any(|route| route == "/api/tasks"));
            }
            other => panic!("expected resync envelope, got {other:?}"),
        }
    }
}
