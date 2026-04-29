//! Helpers split out of `settings_runtime.rs` so the main file stays under
//! the 500-line CI limit. No `SettingsRuntime` state touched here — every
//! function operates over `Config` / `WorkflowRuntimeMode` (plus, where
//! noted, an external `SqlitePool` or filesystem read).

use std::collections::HashSet;
use std::time::Duration;

use sqlx::SqlitePool;

use super::settings_runtime::ApplyConfigError;
use crate::config::{
    captain_workflow_path, scout_workflow_path, CaptainWorkflow, Config, ScoutWorkflow,
};
use crate::service::apply_workflow_mode_overrides;
use crate::types::{ConfigChangeEvent, WorkflowRuntimeMode};

/// Minimum tick interval per mode. Sandbox drops to 1s so the full state
/// machine can be exercised in seconds; prod/dev keep the 10s floor that
/// protects the daemon from runaway ticks.
pub(crate) fn tick_interval_floor_s(mode: WorkflowRuntimeMode) -> u64 {
    match mode {
        WorkflowRuntimeMode::Sandbox => 1,
        WorkflowRuntimeMode::Normal | WorkflowRuntimeMode::Dev => 10,
    }
}

pub(crate) fn clamped_tick_duration(raw: u64, mode: WorkflowRuntimeMode) -> Duration {
    Duration::from_secs(raw.max(tick_interval_floor_s(mode)))
}

pub(crate) fn classify_change(old_config: &Config, new_config: &Config) -> ConfigChangeEvent {
    let telegram_changed = old_config.channels.telegram.enabled
        != new_config.channels.telegram.enabled
        || old_config.channels.telegram.owner != new_config.channels.telegram.owner
        || old_config.channels.telegram.token != new_config.channels.telegram.token
        || old_config.env.get("TELEGRAM_MANDO_BOT_TOKEN")
            != new_config.env.get("TELEGRAM_MANDO_BOT_TOKEN");
    let captain_changed = old_config.captain.auto_schedule != new_config.captain.auto_schedule
        || old_config.captain.tick_interval_s != new_config.captain.tick_interval_s;
    let ui_changed = old_config.ui.open_at_login != new_config.ui.open_at_login;

    let changed: HashSet<ConfigChangeEvent> = [
        telegram_changed.then_some(ConfigChangeEvent::Telegram),
        captain_changed.then_some(ConfigChangeEvent::Captain),
        ui_changed.then_some(ConfigChangeEvent::Ui),
    ]
    .into_iter()
    .flatten()
    .collect();

    let configs_equal = match (
        serde_json::to_value(old_config),
        serde_json::to_value(new_config),
    ) {
        (Ok(left), Ok(right)) => left == right,
        _ => {
            tracing::warn!(
                module = "config",
                "config serialization failed during change classification, treating as changed"
            );
            false
        }
    };
    if changed.is_empty() && configs_equal {
        return ConfigChangeEvent::None;
    }
    match changed.len() {
        1 => changed
            .iter()
            .copied()
            .next()
            .unwrap_or(ConfigChangeEvent::Full),
        _ => ConfigChangeEvent::Full,
    }
}

// SAFETY: env-backed integration keys hot-swap at runtime from this centralized path.
// A wider removal of process-wide env mutation would be a broader runtime-contract change.
pub(crate) fn sync_process_env(
    old_env: &std::collections::HashMap<String, String>,
    new_env: &std::collections::HashMap<String, String>,
) {
    for key in old_env.keys() {
        if !new_env.contains_key(key) {
            unsafe { std::env::remove_var(key) };
        }
    }
    for (key, value) in new_env {
        if old_env.get(key) != Some(value) {
            unsafe { std::env::set_var(key, value) };
        }
    }
}

#[tracing::instrument(skip_all)]
pub(crate) async fn hydrate_projects(
    db_pool: &SqlitePool,
    old_config: &Config,
    new_config: &mut Config,
) {
    if let Err(err) = crate::io::projects::load_into_config(db_pool, new_config).await {
        tracing::warn!(module = "config", error = %err, "failed to reload projects after config save");
        new_config.captain.projects = old_config.captain.projects.clone();
    }
}

pub(crate) fn validate_captain_workflow(config: &Config) -> Result<(), ApplyConfigError> {
    crate::io::config_fs::try_load_captain_workflow(
        &captain_workflow_path(),
        config.captain.tick_interval_s,
    )
    .map(|_| ())
    .map_err(|err| ApplyConfigError::Validation(err.to_string()))
}

pub(crate) fn load_workflows_for_mode(
    config: &mut Config,
    workflow_mode: WorkflowRuntimeMode,
) -> anyhow::Result<(CaptainWorkflow, ScoutWorkflow)> {
    let mut captain_workflow = crate::io::config_fs::load_captain_workflow(
        &captain_workflow_path(),
        config.captain.tick_interval_s,
    )?;
    let mut scout_workflow =
        crate::io::config_fs::load_scout_workflow(&scout_workflow_path(), config)?;
    apply_workflow_mode_overrides(
        workflow_mode,
        config,
        &mut captain_workflow,
        &mut scout_workflow,
    );
    match workflow_mode {
        WorkflowRuntimeMode::Normal => {}
        WorkflowRuntimeMode::Dev => tracing::info!(
            module = "settings-runtime-settings_runtime",
            "dev mode: all models forced to sonnet"
        ),
        WorkflowRuntimeMode::Sandbox => tracing::info!(
            module = "settings-runtime-settings_runtime",
            tick_interval_s = config.captain.tick_interval_s,
            stale_threshold_s = captain_workflow.agent.stale_threshold_s.as_secs(),
            "sandbox mode: models forced to haiku + timing overrides applied"
        ),
    }
    Ok((captain_workflow, scout_workflow))
}
