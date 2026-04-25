use std::path::Path;

use anyhow::Context;
use serde::{Deserialize, Serialize};

use crate::types::{UiDesiredState, UiLaunchSpec};

const REDACTED_ENV_KEYS: [&str; 1] = ["MANDO_AUTH_TOKEN"];

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PersistedUiLaunchSpec {
    pub exec_path: String,
    pub args: Vec<String>,
    pub cwd: Option<String>,
    pub env: std::collections::HashMap<String, String>,
}

impl From<UiLaunchSpec> for PersistedUiLaunchSpec {
    fn from(spec: UiLaunchSpec) -> Self {
        let mut env = spec.env;
        for key in REDACTED_ENV_KEYS {
            env.remove(key);
        }
        Self {
            exec_path: spec.exec_path,
            args: spec.args,
            cwd: spec.cwd,
            env,
        }
    }
}

impl From<PersistedUiLaunchSpec> for UiLaunchSpec {
    fn from(spec: PersistedUiLaunchSpec) -> Self {
        Self {
            exec_path: spec.exec_path,
            args: spec.args,
            cwd: spec.cwd,
            env: spec.env,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PersistedUiState {
    pub desired_state: UiDesiredState,
    pub launch_spec: Option<PersistedUiLaunchSpec>,
}

impl Default for PersistedUiState {
    fn default() -> Self {
        Self {
            desired_state: UiDesiredState::Running,
            launch_spec: None,
        }
    }
}

pub(crate) fn load_state(state_path: &Path) -> anyhow::Result<PersistedUiState> {
    // Fail-fast: the only legitimate "use defaults" path is NotFound
    // (first launch). Any other read error (permission denied, disk
    // issue) and any serde failure propagate so the caller can decide
    // whether to bail or surface a user-visible error, rather than
    // silently resetting `desired_state` to Running.
    match std::fs::read_to_string(state_path) {
        Ok(raw) => serde_json::from_str::<PersistedUiState>(&raw).map_err(|err| {
            anyhow::anyhow!(
                "failed to parse ui-state.json at {}: {err}",
                state_path.display()
            )
        }),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(PersistedUiState::default()),
        Err(err) => Err(anyhow::anyhow!(
            "failed to read ui-state.json at {}: {err}",
            state_path.display()
        )),
    }
}

pub(crate) fn persist_state(
    state_path: &Path,
    desired_state: UiDesiredState,
    launch_spec: Option<UiLaunchSpec>,
) -> anyhow::Result<()> {
    let persisted = PersistedUiState {
        desired_state,
        launch_spec: launch_spec.map(Into::into),
    };
    if let Some(parent) = state_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    std::fs::write(state_path, serde_json::to_vec_pretty(&persisted)?)
        .with_context(|| format!("failed to write {}", state_path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persisted_state_redacts_auth_token() {
        let path = std::env::temp_dir().join(format!(
            "transport-ui-state-{}-{}.json",
            std::process::id(),
            std::thread::current().name().unwrap_or("test")
        ));
        let spec = UiLaunchSpec {
            exec_path: "/tmp/electron".into(),
            args: vec!["main.js".into()],
            cwd: Some("/tmp".into()),
            env: std::collections::HashMap::from([
                ("MANDO_AUTH_TOKEN".into(), "secret-token".into()),
                ("MANDO_GATEWAY_PORT".into(), "18701".into()),
            ]),
        };

        persist_state(&path, UiDesiredState::Running, Some(spec)).unwrap();

        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(!raw.contains("secret-token"));
        assert!(raw.contains("MANDO_GATEWAY_PORT"));
        let _ = std::fs::remove_file(path);
    }
}
