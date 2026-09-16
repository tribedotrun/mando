//! Error types for mando-cc.
//!
//! Public entry points (`CcSession::spawn`, `CcOneShot::run_with_pid_hook`,
//! `spawn_process`, `spawn_detached`) return `Result<_, CcError>` so callers
//! can match on structured failure modes (rate limit, spawn failure, closed
//! stream) without substring parsing error strings. `CcError` derives
//! `thiserror::Error`, so it converts into `anyhow::Error` automatically
//! through the `?` operator for callers that still use `anyhow`.

use std::io;
use std::path::PathBuf;

use thiserror::Error;

/// Coarse classification for `CcError::ApiError` — callers use this to decide
/// whether to retry silently (`Transient`) or surface immediately (`Fatal`).
///
/// Classification is static so it can be applied without external config.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorClass {
    Transient,
    Fatal,
}

/// Errors returned by mando-cc public APIs.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum CcError {
    /// Failed to spawn the `claude` binary.
    #[error("failed to spawn claude binary at {}: {source}", binary.display())]
    SpawnFailed {
        binary: PathBuf,
        #[source]
        source: io::Error,
    },

    /// CC ended its turn with an API error envelope (`is_error: true`). We no
    /// longer launder these into `Ok(CcResult)` — the error goes up as an
    /// error, so callers and observability see the failure.
    ///
    /// `credential_id` identifies the settings-managed credential whose OAuth
    /// token was in use; the failover layer reads it to cool down exactly that
    /// credential on 429s without a reverse lookup. `None` means ambient auth.
    #[error("CC API error (status {api_error_status:?}): {message}")]
    ApiError {
        api_error_status: Option<u16>,
        message: String,
        session_id: String,
        credential_id: Option<i64>,
    },

    /// Every healthy credential is in rate-limit cooldown and the caller
    /// cannot make progress right now. `earliest_reset` is the unix timestamp
    /// (seconds) of the soonest cooldown in the pool. Callers park the work
    /// (e.g. task → Paused) and retry after that time.
    #[error("all credentials rate-limited (earliest reset {earliest_reset})")]
    AllCredentialsExhausted { earliest_reset: i64 },

    /// Mando or the provider stopped the session before successful completion.
    #[error("CC session {session_id} was interrupted")]
    Interrupted { session_id: String },

    /// The CC stream closed unexpectedly (stdin/stdout pipe broke, process
    /// exited without emitting a result envelope).
    #[error("CC stream closed before result")]
    StreamClosed,

    /// `CcConfig` validation failed during build (e.g. missing required
    /// fields, conflicting options).
    #[error("invalid CC config: {0}")]
    InvalidConfig(String),

    /// Generic I/O error (reading streams, opening files, etc.).
    #[error(transparent)]
    Io(#[from] io::Error),

    /// Any other error raised from internal helpers that still use anyhow.
    /// Lets internal code propagate arbitrary failures through the public
    /// boundary without loss of context, while still letting callers match
    /// on the structured variants above for the common cases.
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl CcError {
    /// HTTP-like status surfaced by the Anthropic API when this error variant
    /// carries one. Returned as i64 so it drops straight into a SQLite column.
    pub fn api_error_status(&self) -> Option<i64> {
        match self {
            CcError::ApiError {
                api_error_status, ..
            } => api_error_status.map(|s| s as i64),
            _ => None,
        }
    }

    /// Classify whether this error is worth retrying. Only `ApiError` carries
    /// an HTTP-like status today; every other variant is `Fatal`.
    ///
    /// 429 is NOT transient here. A 429 from CC ("You've hit your limit…")
    /// resets on the account's rate-limit window boundary (up to ~5h away),
    /// so the failover layer (`run_with_credential_failover`) handles it by
    /// cooling down the failing credential and re-picking, rather than any
    /// inner retry loop busy-looping.
    pub fn classify(&self) -> ErrorClass {
        match self {
            CcError::ApiError {
                api_error_status, ..
            } => classify_status(*api_error_status),
            CcError::AllCredentialsExhausted { .. }
            | CcError::Interrupted { .. }
            | CcError::SpawnFailed { .. }
            | CcError::StreamClosed
            | CcError::InvalidConfig(_)
            | CcError::Io(_)
            | CcError::Other(_) => ErrorClass::Fatal,
        }
    }
}

/// Transient = retry makes sense (gateway/capacity errors that recover in
/// seconds); Fatal = retrying will see the same error.
fn classify_status(status: Option<u16>) -> ErrorClass {
    match status {
        Some(502) | Some(503) | Some(504) | Some(529) => ErrorClass::Transient,
        _ => ErrorClass::Fatal,
    }
}
