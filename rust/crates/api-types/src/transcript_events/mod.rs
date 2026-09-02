//! Typed events emitted from CC session JSONL streams.
//!
//! Supersedes the legacy markdown projection (`TranscriptResponse`) and the
//! partial ndjson `TranscriptLine` shape. Each event mirrors exactly what the
//! transcript viewer renders, with named enums for every finite field so the
//! wire contract stays closed-set end-to-end.
//!
//! Parsing is best-effort — anything the daemon cannot recognize becomes an
//! `Unknown` envelope rather than dropping silently. This keeps the renderer
//! honest about coverage gaps instead of letting them rot into empty state.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

mod envelope;
mod messages;
mod result;
mod system;
mod tools;

pub use envelope::*;
pub use messages::*;
pub use result::*;
pub use system::*;
pub use tools::*;

// ── Top-level event envelope ───────────────────────────────────────────

/// One entry in a CC session transcript.
///
/// Adjacently tagged by `kind` + `data` — matches the repo's SSE envelope
/// pattern so inner structs can keep `deny_unknown_fields` strict.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "kind", content = "data", rename_all = "snake_case")]
pub enum TranscriptEvent {
    SystemInit(SystemInitEvent),
    SystemCompactBoundary(SystemCompactBoundaryEvent),
    SystemStatus(SystemStatusEvent),
    SystemApiRetry(SystemApiRetryEvent),
    SystemLocalCommandOutput(SystemLocalCommandOutputEvent),
    SystemHook(SystemHookEvent),
    SystemRateLimit(SystemRateLimitEvent),
    SystemThinkingTokens(SystemThinkingTokensEvent),
    SystemClaudeProgress(SystemClaudeProgressEvent),
    SystemTokenUsage(SystemTokenUsageEvent),
    User(UserEvent),
    Assistant(AssistantEvent),
    ToolProgress(ToolProgressEvent),
    Result(ResultEvent),
    /// A CC JSONL line Mando could not classify. The `raw` field carries the
    /// original line (stringified JSON) so the renderer can still surface it
    /// rather than silently drop. Catalogued escape — see
    /// `.ai/guardrail-allowlists/internal-value.txt` under `transcript-events`.
    Unknown(UnknownEvent),
}

// ── Common fields ──────────────────────────────────────────────────────

/// Offset of an event within the source JSONL file. Stable across parses,
/// so the renderer can key virtualized rows without needing uuid fallbacks.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EventIndex {
    #[ts(type = "number")]
    pub line_number: u32,
}

/// Session-branching metadata shared by every event.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EventMeta {
    pub index: EventIndex,
    pub uuid: Option<String>,
    pub parent_uuid: Option<String>,
    pub session_id: Option<String>,
    pub timestamp: Option<String>,
    pub is_sidechain: Option<bool>,
}

impl TranscriptEvent {
    /// Line number of this event in the source JSONL file. Every variant
    /// carries an `EventMeta` with the same field path, so callers can
    /// centralize the lookup here instead of re-matching the enum.
    pub fn line_number(&self) -> u32 {
        self.meta().index.line_number
    }

    /// Branching metadata for this event, regardless of variant.
    pub fn meta(&self) -> &EventMeta {
        match self {
            TranscriptEvent::SystemInit(e) => &e.meta,
            TranscriptEvent::SystemCompactBoundary(e) => &e.meta,
            TranscriptEvent::SystemStatus(e) => &e.meta,
            TranscriptEvent::SystemApiRetry(e) => &e.meta,
            TranscriptEvent::SystemLocalCommandOutput(e) => &e.meta,
            TranscriptEvent::SystemHook(e) => &e.meta,
            TranscriptEvent::SystemRateLimit(e) => &e.meta,
            TranscriptEvent::SystemThinkingTokens(e) => &e.meta,
            TranscriptEvent::SystemClaudeProgress(e) => &e.meta,
            TranscriptEvent::SystemTokenUsage(e) => &e.meta,
            TranscriptEvent::User(e) => &e.meta,
            TranscriptEvent::Assistant(e) => &e.meta,
            TranscriptEvent::ToolProgress(e) => &e.meta,
            TranscriptEvent::Result(e) => &e.meta,
            TranscriptEvent::Unknown(e) => &e.meta,
        }
    }
}
