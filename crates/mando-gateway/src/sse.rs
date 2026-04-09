//! /api/events — Server-Sent Events endpoint.

use std::convert::Infallible;
use std::time::Duration;

use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures_util::stream::Stream;
use serde_json::json;
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
        Ok(value) => Event::default().data(value.to_string()),
        Err(e) => {
            tracing::error!(error = %e, "SSE snapshot: failed to build snapshot");
            let payload = json!({
                "event": "snapshot_error",
                "data": {
                    "message": format!("snapshot build failed: {e}"),
                    "retry": true,
                },
            });
            // Use a custom event name (not "error") so it doesn't collide with
            // EventSource's built-in "error" event, which fires on native
            // connection failures and dispatches a plain Event (no data). A
            // named SSE event is dispatched as a MessageEvent only to listeners
            // registered for that exact name.
            Event::default()
                .event("snapshot_error")
                .data(payload.to_string())
        }
    };
    let snapshot_stream = futures_util::stream::once(async { Ok(snapshot_event) });

    // Live event stream (incremental deltas).
    let rx = state.bus.subscribe();
    let stream = BroadcastStream::new(rx);

    let event_stream = stream.filter_map(|result| {
        match result {
            Ok((bus_event, data)) => {
                let event_name = serde_json::to_value(bus_event)
                    .ok()
                    .and_then(|v| v.as_str().map(String::from))
                    .unwrap_or_else(|| "unknown".into());

                let ts = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or(Duration::ZERO)
                    .as_secs_f64();

                let payload = json!({
                    "event": event_name,
                    "ts": ts,
                    "data": data,
                });

                let event = Event::default().data(payload.to_string());

                Some(Ok(event))
            }
            Err(lag_err) => {
                // Slow consumer: the broadcast channel dropped events.
                // Emit a named `resync` event so the client reloads full
                // state via the REST APIs instead of silently missing
                // messages. Using a dedicated event name (not the default
                // SSE message channel) lets the renderer hook up a specific
                // addEventListener without polluting the normal bus stream.
                let skipped = format!("{lag_err}");
                tracing::warn!(
                    module = "sse",
                    error = %skipped,
                    "SSE client lagged, broadcast events dropped"
                );
                let ts = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or(Duration::ZERO)
                    .as_secs_f64();
                let payload = json!({
                    "event": "resync",
                    "ts": ts,
                    "data": {
                        "reason": skipped,
                        "reload": ["/api/tasks", "/api/sessions", "/api/workers"],
                    },
                });
                Some(Ok(Event::default()
                    .event("resync")
                    .data(payload.to_string())))
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

/// Build a full state snapshot for the initial SSE event.
async fn build_snapshot(state: &AppState) -> anyhow::Result<serde_json::Value> {
    let workflow = state.captain_workflow.load_full();

    // Task items + active workers from the TaskStore.
    // Load all items once and build a lookup map — avoids N+1 find_by_id calls.
    let store = state.task_store.read().await;
    let all_items = store.load_all().await?;
    drop(store);
    let tasks = serde_json::to_value(&all_items)?;

    let workers = {
        let health_path = mando_config::worker_health_path();
        let health = mando_captain::io::health_store::load_health_state_async(&health_path).await?;
        let nudge_budget = workflow.agent.max_interventions;

        all_items
            .iter()
            .filter(|t| {
                matches!(
                    t.status,
                    mando_types::task::ItemStatus::InProgress
                        | mando_types::task::ItemStatus::CaptainReviewing
                        | mando_types::task::ItemStatus::CaptainMerging
                ) && t.worker.is_some()
            })
            .map(|task| {
                let worker_name = task.worker.as_deref().unwrap_or("");
                let nudge_count = mando_captain::io::health_store::get_health_u32(
                    &health,
                    worker_name,
                    "nudge_count",
                );
                let last_action = health
                    .get(worker_name)
                    .and_then(|v| v.get("last_action"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                json!({
                    "id": task.id,
                    "title": task.title,
                    "status": task.status.as_str(),
                    "worker": task.worker,
                    "project": task.project,
                    "worktree": task.worktree,
                    "branch": task.branch,
                    "pr_number": task.pr_number,
                    "started_at": task.worker_started_at,
                    "last_activity_at": task.last_activity_at,
                    "cc_session_id": task.session_ids.worker,
                    "intervention_count": task.intervention_count,
                    "nudge_count": nudge_count,
                    "nudge_budget": nudge_budget,
                    "last_action": last_action,
                })
            })
            .collect::<Vec<_>>()
    };

    // Workbenches (active, non-archived, non-deleted).
    let workbenches = match mando_db::queries::workbenches::load_active(state.db.pool()).await {
        Ok(wbs) => serde_json::to_value(&wbs).unwrap_or_default(),
        Err(e) => {
            tracing::warn!(error = %e, "SSE snapshot: failed to load workbenches");
            serde_json::Value::Array(vec![])
        }
    };

    // Terminal sessions (in-memory, from TerminalHost).
    let terminals = serde_json::to_value(state.terminal_host.list()).unwrap_or_default();

    // Config (current daemon config).
    let config = match serde_json::to_value(&*state.config.load_full()) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(error = %e, "SSE snapshot: failed to serialize config");
            serde_json::Value::Null
        }
    };

    // Daemon info.
    let uptime = state.start_time.elapsed().as_secs();
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs_f64();

    Ok(json!({
        "event": "snapshot",
        "ts": ts,
        "data": {
            "tasks": tasks,
            "workers": workers,
            "workbenches": workbenches,
            "terminals": terminals,
            "config": config,
            "daemon": {
                "version": env!("CARGO_PKG_VERSION"),
                "uptime": uptime,
            }
        }
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mando_types::BusEvent;

    #[tokio::test]
    async fn sse_receives_event() {
        let bus = mando_shared::EventBus::new();
        let mut rx = bus.subscribe();

        bus.send(BusEvent::Tasks, Some(json!({"test": true})));

        let (event, data) = rx.recv().await.unwrap();
        assert_eq!(event, BusEvent::Tasks);
        assert!(data.is_some());
    }
}
