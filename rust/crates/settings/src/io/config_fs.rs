//! Filesystem I/O for config, workflow, and logo files.
//!
//! Calls into `crate::config` for pure parsing; this module owns all `std::fs` calls.

use std::path::{Path, PathBuf};

use crate::config::error::ConfigError;
use crate::config::settings::Config;

/// Load configuration from file, falling back to defaults if the file doesn't exist.
pub fn load_config(path: Option<&Path>) -> Result<Config, ConfigError> {
    let path = path
        .map(PathBuf::from)
        .unwrap_or_else(crate::config::loader::get_config_path);

    if path.exists() {
        let content = std::fs::read_to_string(&path).map_err(|e| ConfigError::Io {
            op: "read".into(),
            path: path.clone(),
            source: e,
        })?;
        crate::config::loader::parse_config(&content, &path)
    } else {
        let mut config = Config::default();
        config.populate_runtime_fields();
        Ok(config)
    }
}

/// Save configuration to a JSON file.
pub fn save_config(config: &Config, path: Option<&Path>) -> Result<(), std::io::Error> {
    let path = path
        .map(PathBuf::from)
        .unwrap_or_else(crate::config::loader::get_config_path);

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let json = crate::config::loader::serialize_config(config)?;
    std::fs::write(&path, json)
}

/// Load captain workflow: user override at `path` if it exists, else compiled-in default.
/// Panics on invalid template keys (fail-fast at startup).
pub fn load_captain_workflow(
    override_path: &Path,
    tick_interval_s: u64,
) -> Result<crate::config::workflow::CaptainWorkflow, ConfigError> {
    let content = read_override(override_path)?;
    if content.is_some() {
        tracing::info!(
            module = "settings-io-config_fs",
            "loaded captain workflow from {}",
            override_path.display()
        );
    }
    let wf = crate::config::workflow::parse_captain_workflow_or_default(
        content.as_deref(),
        override_path,
    )?;
    crate::config::workflow_validate::validate_captain_workflow(&wf);
    crate::config::workflow_validate::validate_agent_config(&wf.agent, tick_interval_s);
    Ok(wf)
}

/// Non-panicking variant for use in HTTP handlers.
pub fn try_load_captain_workflow(
    override_path: &Path,
    tick_interval_s: u64,
) -> Result<crate::config::workflow::CaptainWorkflow, ConfigError> {
    let content = read_override(override_path)?;
    let wf = crate::config::workflow::parse_captain_workflow_or_default(
        content.as_deref(),
        override_path,
    )?;
    if let Err(e) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        crate::config::workflow_validate::validate_captain_workflow(&wf);
    })) {
        let msg = e
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| e.downcast_ref::<&str>().map(|s| (*s).to_string()))
            .unwrap_or_else(|| "workflow validation failed".to_string());
        return Err(ConfigError::Validation(msg));
    }
    crate::config::workflow_validate::try_validate_agent_config(&wf.agent, tick_interval_s)
        .map_err(ConfigError::Validation)?;
    Ok(wf)
}

/// Load scout workflow: user override at `path` if it exists, else compiled-in default.
pub fn load_scout_workflow(
    override_path: &Path,
    config: &Config,
) -> Result<crate::config::workflow::ScoutWorkflow, ConfigError> {
    let content = read_override(override_path)?;
    if content.is_some() {
        tracing::info!(
            module = "settings-io-config_fs",
            "loaded scout workflow from {}",
            override_path.display()
        );
    }
    crate::config::workflow::parse_scout_workflow_or_default(
        content.as_deref(),
        override_path,
        config,
    )
}

/// Read an override file if it exists, returning `None` if absent.
fn read_override(path: &Path) -> Result<Option<String>, ConfigError> {
    if path.exists() {
        let contents = std::fs::read_to_string(path).map_err(|e| ConfigError::Io {
            op: "read".into(),
            path: path.to_path_buf(),
            source: e,
        })?;
        Ok(Some(contents))
    } else {
        Ok(None)
    }
}
