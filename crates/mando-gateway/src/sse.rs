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
    // Build initial snapshot from current state.
    let snapshot = build_snapshot(&state).await;
    let snapshot_event = Event::default().data(snapshot.to_string());
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
            Err(_) => {
                // Lagged — skip missed events.
                None
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
async fn build_snapshot(state: &AppState) -> serde_json::Value {
    let workflow = state.captain_workflow.load_full();

    // Task items + active workers from the TaskStore.
    // Load all items once and build a lookup map — avoids N+1 find_by_id calls.
    let store = state.task_store.read().await;
    let all_items = store.load_all().await.unwrap_or_else(|e| {
        tracing::error!(error = %e, "SSE snapshot: failed to load tasks from DB");
        Vec::new()
    });
    drop(store);
    let tasks = serde_json::to_value(&all_items).unwrap_or_else(|e| {
        tracing::warn!(error = %e, "failed to serialize task items");
        json!([])
    });

    let workers = {
        let health_path = mando_config::worker_health_path();
        let health = mando_captain::io::health_store::load_health_state(&health_path);
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
                    "worker": task.worker,
                    "project": task.project,
                    "worktree": task.worktree,
                    "branch": task.branch,
                    "pr": task.pr,
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

    // Cron jobs.
    let cron_jobs = {
        let cs = state.cron_service.read().await;
        let jobs = cs.list_jobs(true);
        serde_json::to_value(&jobs).unwrap_or_else(|e| {
            tracing::warn!(error = %e, "failed to serialize cron jobs");
            json!([])
        })
    };

    // Daemon info.
    let uptime = state.start_time.elapsed().as_secs();
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs_f64();

    json!({
        "event": "snapshot",
        "ts": ts,
        "data": {
            "tasks": tasks,
            "workers": workers,
            "cronJobs": cron_jobs,
            "daemon": {
                "version": env!("CARGO_PKG_VERSION"),
                "uptime": uptime,
            }
        }
    })
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
