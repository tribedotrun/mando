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

    /// CC reported a hard rate-limit rejection. `resets_at` is the unix
    /// timestamp at which the rate limit window resets, if known.
    #[error("rate limit rejected: {message}")]
    RateLimit {
        resets_at: Option<i64>,
        message: String,
    },

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
