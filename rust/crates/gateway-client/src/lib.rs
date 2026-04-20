//! Reusable typed daemon client.
//!
//! Every Rust caller that speaks to the mando daemon uses this crate:
//! - `parse_sse_block` — decodes a single SSE `data:` block into the typed
//!   `api_types::SseEnvelope` (no `serde_json::Value` hops).
//! - [`SseConsumer`] — end-to-end SSE subscriber with reconnect/backoff.
//!
//! The #882 drift (wire consumer parsing nested envelopes by hand) is
//! prevented structurally: there is exactly one decode path and it returns
//! the `api_types` envelope directly.

mod sse;

pub use sse::{parse_sse_block, ParseError, SseConsumer, SseSignal};
