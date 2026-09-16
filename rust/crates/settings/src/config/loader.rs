//! Config file resolution and pure parsing/serialization.
//!
//! Filesystem I/O lives in `crate::io::config_fs` — this module handles
//! only path resolution and JSON (de)serialization.

use std::path::PathBuf;

use serde_json::Value;

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

/// Parse a config JSON string into a `Config`, filling in defaults for any
/// missing fields, then populate runtime-only fields.
///
/// The workspace-wide ban on `#[serde(default)]` means a raw
/// `serde_json::from_str::<Config>` fails the moment the user's file lacks a
/// single field. User-facing `config.json` must stay forward-compatible: a
/// user who omits a new field (or is on an older install) should keep
/// booting. We layer the user JSON onto `Config::default()` JSON so every
/// missing field resolves to its default.
pub fn parse_config(content: &str, path: &std::path::Path) -> Result<Config, ConfigError> {
    let user: Value = serde_json::from_str(content).map_err(|e| ConfigError::JsonParse {
        path: path.to_path_buf(),
        source: e,
    })?;
    let defaults = serde_json::to_value(Config::default()).map_err(|e| ConfigError::JsonParse {
        path: path.to_path_buf(),
        source: e,
    })?;
    let merged = merge_into(defaults, user);
    let mut config =
        serde_json::from_value::<Config>(merged).map_err(|e| ConfigError::JsonParse {
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

/// Deep-merge `overlay` onto `base`: objects merge key-by-key (recursing on
/// shared keys), anything else at a given path is replaced by `overlay`.
/// Overlay `null` at any level is a no-op: the top-level arm keeps `base`
/// intact, and inside objects a `null` value means "skip this key" so a
/// user `null` never erases a default.
fn merge_into(base: Value, overlay: Value) -> Value {
    match (base, overlay) {
        (base, Value::Null) => base,
        (Value::Object(mut base_map), Value::Object(overlay_map)) => {
            for (k, v) in overlay_map {
                if matches!(v, Value::Null) {
                    continue;
                }
                let merged = match base_map.remove(&k) {
                    Some(existing) => merge_into(existing, v),
                    None => v,
                };
                base_map.insert(k, merged);
            }
            Value::Object(base_map)
        }
        (_, overlay) => overlay,
    }
}
