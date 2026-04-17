//! Config file resolution and pure parsing/serialization.
//!
//! Filesystem I/O lives in `crate::io::config_fs` — this module handles
//! only path resolution and JSON (de)serialization.

use std::path::PathBuf;

use super::error::ConfigError;
use super::settings::Config;
use global_infra::paths::expand_tilde;

/// Return the config file path (`MANDO_CONFIG` env or `~/.mando/config.json`).
pub fn get_config_path() -> PathBuf {
    if let Ok(v) = std::env::var("MANDO_CONFIG") {
        return expand_tilde(&v);
    }
    global_infra::paths::data_dir().join("config.json")
}

/// Parse a config JSON string into a `Config` struct, then populate runtime fields.
pub fn parse_config(content: &str, path: &std::path::Path) -> Result<Config, ConfigError> {
    let mut config =
        serde_json::from_str::<Config>(content).map_err(|e| ConfigError::JsonParse {
            path: path.to_path_buf(),
            source: e,
        })?;
    config.populate_runtime_fields();
    Ok(config)
}

/// Serialize a `Config` struct to pretty-printed JSON.
pub fn serialize_config(config: &Config) -> Result<String, std::io::Error> {
    serde_json::to_string_pretty(config).map_err(std::io::Error::other)
}
