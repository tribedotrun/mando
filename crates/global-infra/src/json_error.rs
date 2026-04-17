//! Error types for mando-shared.
//!
//! Public helpers (`load_json_file`, `save_json_file`) surface these typed
//! errors instead of untyped `anyhow::Error` so callers can match on failure
//! modes (missing vs. corrupt vs. permission denied) without string parsing.
//! `SharedError` implements `From<SharedError> for anyhow::Error` via
//! `thiserror`, so callers that still propagate with `anyhow` get lossless
//! conversion through the `?` operator.

use std::io;
use std::path::PathBuf;

use thiserror::Error;

/// Errors returned by mando-shared helpers.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum SharedError {
    /// Generic I/O failure when reading or writing a JSON file.
    #[error("{op} failed for {}: {source}", path.display())]
    Io {
        /// The operation that failed, e.g. `"read"`, `"create"`, `"rename"`,
        /// `"fsync"`. Lets callers disambiguate without inspecting the source.
        op: String,
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    /// JSON parse failure when loading a file.
    #[error("failed to parse JSON at {}: {source}", path.display())]
    JsonParse {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },

    /// JSON serialization failure (save path).
    #[error("failed to serialize JSON: {0}")]
    Serialization(#[from] serde_json::Error),
}
