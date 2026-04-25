//! `/api/sessions/{id}/events/stream` — live SSE tail of a CC session's
//! typed transcript events.
//!
//! Lifecycle:
//! 1. Load the snapshot (all events currently in the JSONL file plus the
//!    byte offset and line number to resume tailing from).
//! 2. Emit a single `snapshot` envelope carrying those events, followed by a
//!    `snapshot_complete` sentinel.
//! 3. If the session is still running, poll the file every ~500ms, parse any
//!    new events from the byte offset, and emit each as an `event` envelope.
//! 4. When the session finishes (result event observed, meta status flips, or
//!    client disconnects), emit `connection_closed`.
//!
//! Uses `tokio::time::interval` rather than the `notify` crate: polling is
//! cheap (metadata + tail read from an offset), works identically on macOS
//! and Linux, and keeps the dep graph flat.

use std::convert::Infallible;
use std::path::PathBuf;
use std::time::Duration;

use axum::extract::{Path, State};
use axum::response::sse::{Event, KeepAlive, Sse};
use futures_util::stream::Stream;
use tokio::sync::mpsc;

use crate::AppState;

const POLL_INTERVAL: Duration = Duration::from_millis(500);
const MAX_STREAM_DURATION: Duration = Duration::from_secs(60 * 60 * 4);
const SNAPSHOT_MISSING_REASON: &str = "session not found";

/// GET /api/sessions/{id}/events/stream
#[crate::instrument_api(method = "GET", path = "/api/sessions/{id}/events/stream")]
pub(crate) async fn get_session_events_stream(
    State(state): State<AppState>,
    Path(api_types::SessionIdParams { id }): Path<api_types::SessionIdParams>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let (tx, rx) = mpsc::unbounded_channel::<Result<Event, Infallible>>();
    let stream = tokio_stream::wrappers::UnboundedReceiverStream::new(rx);

    tokio::spawn(async move {
        run_tail(state, id, tx).await;
    });

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("heartbeat"),
    )
}

async fn run_tail(
    state: AppState,
    session_id: String,
    tx: mpsc::UnboundedSender<Result<Event, Infallible>>,
) {
    let snapshot = match state.sessions.events_snapshot(&session_id).await {
        Ok(Some(s)) => s,
        Ok(None) => {
            global_infra::best_effort!(
                tx.send(encode_error(SNAPSHOT_MISSING_REASON)),
                "send snapshot-missing error to SSE client",
            );
            global_infra::best_effort!(
                tx.send(encode_closed(SNAPSHOT_MISSING_REASON)),
                "send snapshot-missing close to SSE client",
            );
            return;
        }
        Err(e) => {
            tracing::error!(module = "transport-http-sse-session-events", error = %e, session_id = %session_id, "failed to load events snapshot for SSE");
            global_infra::best_effort!(
                tx.send(encode_error(&format!("snapshot load failed: {e}"))),
                "send snapshot-load error to SSE client",
            );
            global_infra::best_effort!(
                tx.send(encode_closed("snapshot load failed")),
                "send snapshot-load close to SSE client",
            );
            return;
        }
    };

    let is_running = snapshot.is_running;
    let stream_path = snapshot.stream_path.clone();
    let mut byte_offset = snapshot.byte_offset;
    let mut next_line = snapshot.next_line;

    let snapshot_batch = api_types::TranscriptSnapshotBatch {
        events: snapshot.events,
    };
    if tx
        .send(encode_envelope(
            &api_types::TranscriptEventEnvelope::Snapshot(Box::new(snapshot_batch)),
        ))
        .is_err()
    {
        return;
    }
    if tx
        .send(encode_envelope(
            &api_types::TranscriptEventEnvelope::SnapshotComplete(
                api_types::TranscriptSnapshotComplete { is_running },
            ),
        ))
        .is_err()
    {
        return;
    }

    let Some(stream_path) = stream_path else {
        global_infra::best_effort!(
            tx.send(encode_closed("no stream file")),
            "send no-stream close to SSE client",
        );
        return;
    };
    if !is_running {
        global_infra::best_effort!(
            tx.send(encode_closed("session finished before stream opened")),
            "send finished-before-open close to SSE client",
        );
        return;
    }

    poll_loop(
        session_id,
        stream_path,
        &tx,
        &mut byte_offset,
        &mut next_line,
    )
    .await;
}

async fn poll_loop(
    session_id: String,
    stream_path: PathBuf,
    tx: &mpsc::UnboundedSender<Result<Event, Infallible>>,
    byte_offset: &mut u64,
    next_line: &mut u32,
) {
    let mut ticker = tokio::time::interval(POLL_INTERVAL);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let started = std::time::Instant::now();

    loop {
        ticker.tick().await;

        // Parse new events and check session-finished meta inside the same
        // blocking task to avoid re-entering the tokio runtime with sync file
        // I/O twice per tick.
        let (events, new_offset, session_finished) = tokio::task::spawn_blocking({
            let stream_path = stream_path.clone();
            let session_id = session_id.clone();
            let offset = *byte_offset;
            let line = *next_line;
            move || {
                let (events, new_offset) =
                    global_claude::parse_events_from_offset(&stream_path, offset, line);
                let finished = global_claude::is_session_finished(&session_id);
                (events, new_offset, finished)
            }
        })
        .await
        .unwrap_or_else(|e| {
            tracing::error!(
                module = "transport-http-sse-session-events",
                error = %e,
                session_id = %session_id,
                "spawn_blocking for event tail panicked",
            );
            (Vec::new(), *byte_offset, false)
        });

        let mut disconnect = false;
        for event in events {
            *next_line = next_line.saturating_add(1);
            let is_result = matches!(event, api_types::TranscriptEvent::Result(_));
            let envelope = api_types::TranscriptEventEnvelope::Event(Box::new(event));
            if tx.send(encode_envelope(&envelope)).is_err() {
                disconnect = true;
                break;
            }
            if is_result {
                global_infra::best_effort!(
                    tx.send(encode_closed("session result received")),
                    "send result-received close to SSE client",
                );
                return;
            }
        }
        if disconnect {
            return;
        }
        *byte_offset = new_offset;

        if started.elapsed() > MAX_STREAM_DURATION {
            global_infra::best_effort!(
                tx.send(encode_closed("stream duration limit reached")),
                "send duration-limit close to SSE client",
            );
            return;
        }
        if !session_finished {
            continue;
        }
        // Session meta says finished — drain one more read and close.
        let (final_events, final_offset) = tokio::task::spawn_blocking({
            let stream_path = stream_path.clone();
            let offset = *byte_offset;
            let line = *next_line;
            move || global_claude::parse_events_from_offset(&stream_path, offset, line)
        })
        .await
        .unwrap_or_else(|_| (Vec::new(), *byte_offset));
        for event in final_events {
            *next_line = next_line.saturating_add(1);
            let envelope = api_types::TranscriptEventEnvelope::Event(Box::new(event));
            if tx.send(encode_envelope(&envelope)).is_err() {
                return;
            }
        }
        *byte_offset = final_offset;
        global_infra::best_effort!(
            tx.send(encode_closed("session finished")),
            "send session-finished close to SSE client",
        );
        return;
    }
}

fn encode_envelope(envelope: &api_types::TranscriptEventEnvelope) -> Result<Event, Infallible> {
    match serde_json::to_string(envelope) {
        Ok(s) => Ok(Event::default().data(s)),
        Err(e) => {
            tracing::error!(
                module = "transport-http-sse-session-events",
                %e,
                "failed to encode transcript event envelope",
            );
            encode_error_bare(&format!("encode failure: {e}"))
        }
    }
}

/// Serialise a stream-error envelope without recursing through
/// `encode_envelope` (a failure there would otherwise loop on itself).
/// Falls back to a hand-rolled JSON literal so the client always sees a
/// typed Error envelope rather than a bare `{}`.
fn encode_error_bare(reason: &str) -> Result<Event, Infallible> {
    let error = api_types::TranscriptStreamError {
        message: reason.to_string(),
        retry: false,
    };
    let envelope = api_types::TranscriptEventEnvelope::Error(error);
    match serde_json::to_string(&envelope) {
        Ok(s) => Ok(Event::default().data(s)),
        Err(_) => Ok(Event::default().data(format!(
            r#"{{"event":"error","data":{{"message":{:?},"retry":false}}}}"#,
            reason
        ))),
    }
}

fn encode_error(reason: &str) -> Result<Event, Infallible> {
    encode_error_bare(reason)
}

fn encode_closed(reason: &str) -> Result<Event, Infallible> {
    encode_envelope(&api_types::TranscriptEventEnvelope::ConnectionClosed(
        api_types::TranscriptConnectionClosed {
            reason: reason.to_string(),
        },
    ))
}
