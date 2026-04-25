//! Pure helpers split out of `settings_runtime.rs` so the main file stays under
//! the 500-line CI limit. No `SettingsRuntime` state touched here — every
//! function is a pure transformation over `Config` / `WorkflowRuntimeMode`.

use std::collections::HashSet;
use std::time::Duration;

use crate::config::Config;
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
