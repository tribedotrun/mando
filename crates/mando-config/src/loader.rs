//! Config file resolution, loading, and saving.

use std::path::{Path, PathBuf};

use crate::error::ConfigError;
use crate::paths::expand_tilde;
use crate::settings::Config;

/// Return the config file path (`MANDO_CONFIG` env or `~/.mando/config.json`).
pub fn get_config_path() -> PathBuf {
    if let Ok(v) = std::env::var("MANDO_CONFIG") {
        return expand_tilde(&v);
    }
    crate::paths::data_dir().join("config.json")
}

/// Load configuration from file.
///
/// Returns `Config::default()` only when the file does not exist. Any read
/// or parse failure is surfaced as an error so the caller can fail loudly
/// instead of silently running with defaults.
///
/// After loading, populates runtime fields (e.g. Telegram tokens from `env`).
pub fn load_config(path: Option<&Path>) -> Result<Config, ConfigError> {
    let path = path.map(PathBuf::from).unwrap_or_else(get_config_path);

    let mut config = if path.exists() {
        let content = std::fs::read_to_string(&path).map_err(|e| ConfigError::Io {
            op: "read".into(),
            path: path.clone(),
            source: e,
        })?;
        serde_json::from_str::<Config>(&content).map_err(|e| ConfigError::JsonParse {
            path: path.clone(),
            source: e,
        })?
    } else {
        Config::default()
    };

    config.populate_runtime_fields();
    Ok(config)
}

/// Save configuration to a JSON file.
pub fn save_config(config: &Config, path: Option<&Path>) -> Result<(), std::io::Error> {
    let path = path.map(PathBuf::from).unwrap_or_else(get_config_path);

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let json = serde_json::to_string_pretty(config).map_err(std::io::Error::other)?;
    std::fs::write(&path, json)
}
