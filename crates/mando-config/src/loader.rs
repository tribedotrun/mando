//! Config file resolution, loading, and saving.

use std::path::{Path, PathBuf};

use crate::paths::expand_tilde;
use crate::settings::Config;

/// Return the config file path (`MANDO_CONFIG` env or `~/.mando/config.json`).
pub fn get_config_path() -> PathBuf {
    if let Ok(v) = std::env::var("MANDO_CONFIG") {
        return expand_tilde(&v);
    }
    crate::paths::data_dir().join("config.json")
}

/// Load configuration from file, falling back to `Config::default()`.
///
/// After loading, populates runtime fields (e.g. Telegram tokens from `env`).
pub fn load_config(path: Option<&Path>) -> Config {
    let path = path.map(PathBuf::from).unwrap_or_else(get_config_path);

    let mut config = if path.exists() {
        match std::fs::read_to_string(&path) {
            Ok(content) => match serde_json::from_str::<Config>(&content) {
                Ok(cfg) => cfg,
                Err(e) => {
                    eprintln!(
                        "Warning: Failed to parse config from {}: {}",
                        path.display(),
                        e,
                    );
                    Config::default()
                }
            },
            Err(e) => {
                eprintln!(
                    "Warning: Failed to read config from {}: {}",
                    path.display(),
                    e,
                );
                Config::default()
            }
        }
    } else {
        Config::default()
    };

    config.populate_runtime_fields();
    config
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
