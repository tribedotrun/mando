//! Error types for mando-config.
//!
//! Loader entry points (`load_config`, `load_captain_workflow`,
//! `load_scout_workflow`) return `Result<_, ConfigError>` so callers can tell
//! apart IO errors (file missing / permission denied), JSON/YAML parse
//! failures, template errors, and semantic validation failures without
//! inspecting error strings. `ConfigError` derives `thiserror::Error`, so it
//! converts into `anyhow::Error` automatically for callers still using
//! `anyhow` via the `?` operator.

use std::io;
use std::path::PathBuf;

use thiserror::Error;

/// Errors returned by mando-config loader entry points.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ConfigError {
    /// File IO error (read / write / create) with the offending path.
    #[error("{op} failed for {}: {source}", path.display())]
    Io {
        op: String,
        path: PathBuf,
        #[source]
        source: io::Error,
    },

    /// `config.json` failed to parse as JSON.
    #[error("failed to parse JSON config at {}: {source}", path.display())]
    JsonParse {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },

    /// A workflow YAML file failed to parse.
    #[error("failed to parse YAML at {}: {source}", path.display())]
    YamlParse {
        path: PathBuf,
        #[source]
        source: serde_yaml::Error,
    },

    /// A MiniJinja template had a syntax error.
    #[error("template syntax error in {template}: {message}")]
    TemplateSyntax { template: String, message: String },

    /// A MiniJinja template failed at render time (e.g. runtime type errors).
    #[error("template render error in {template}: {message}")]
    TemplateRender { template: String, message: String },

    /// A required named template was not present in the workflow map.
    #[error("template not found: {0}")]
    TemplateNotFound(String),

    /// Semantic validation of config/workflow failed (e.g. non-positive
    /// tick interval, budget below limit).
    #[error("validation failed: {0}")]
    Validation(String),
}
