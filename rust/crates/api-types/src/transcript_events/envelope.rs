//! HTTP + SSE wrappers for the transcript events endpoints.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::TranscriptEvent;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TranscriptEventsResponse {
    pub session_id: String,
    pub events: Vec<TranscriptEvent>,
    pub is_running: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TranscriptSnapshotBatch {
    pub events: Vec<TranscriptEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TranscriptSnapshotComplete {
    pub is_running: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TranscriptConnectionClosed {
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TranscriptStreamError {
    pub message: String,
    pub retry: bool,
}

/// SSE envelope for `/api/sessions/{id}/events/stream`. Emits in order:
/// `snapshot` with any existing events, `snapshot_complete` sentinel, then
/// `event` frames as they land in the JSONL file, and finally
/// `connection_closed` on session end or client unsubscribe.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "event", content = "data", rename_all = "snake_case")]
pub enum TranscriptEventEnvelope {
    Snapshot(Box<TranscriptSnapshotBatch>),
    SnapshotComplete(TranscriptSnapshotComplete),
    Event(Box<TranscriptEvent>),
    ConnectionClosed(TranscriptConnectionClosed),
    Error(TranscriptStreamError),
}
