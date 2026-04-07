use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use arc_swap::ArcSwap;
use tokio::sync::{broadcast, watch, Mutex};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConfigChangeEvent {
    None,
    Telegram,
    Captain,
    Ui,
    Full,
}

#[derive(Clone)]
pub struct ConfigManager {
    config: Arc<ArcSwap<mando_config::Config>>,
    write_mu: Arc<Mutex<()>>,
    changes_tx: broadcast::Sender<ConfigChangeEvent>,
    tick_tx: watch::Sender<Duration>,
}

impl ConfigManager {
    pub fn new(
        config: Arc<ArcSwap<mando_config::Config>>,
        write_mu: Arc<Mutex<()>>,
        tick_tx: watch::Sender<Duration>,
    ) -> Self {
        let (changes_tx, _) = broadcast::channel(64);
        Self {
            config,
            write_mu,
            changes_tx,
            tick_tx,
        }
    }

    pub fn load_full(&self) -> Arc<mando_config::Config> {
        self.config.load_full()
    }

    pub fn subscribe_tick(&self) -> watch::Receiver<Duration> {
        self.tick_tx.subscribe()
    }

    pub async fn update<F>(&self, mutator: F) -> anyhow::Result<ConfigChangeEvent>
    where
        F: FnOnce(&mut mando_config::Config) -> anyhow::Result<()>,
    {
        let _guard = self.write_mu.lock().await;
        let old = (*self.config.load_full()).clone();
        let mut new = old.clone();
        mutator(&mut new)?;
        self.commit_locked(old, new).await
    }

    pub async fn replace(
        &self,
        mut new: mando_config::Config,
    ) -> anyhow::Result<ConfigChangeEvent> {
        let _guard = self.write_mu.lock().await;
        let old = (*self.config.load_full()).clone();
        new.populate_runtime_fields();
        self.commit_locked(old, new).await
    }

    /// Replace config and run a synchronous callback under the same write lock.
    /// Use this when related state (e.g. workflows) must be published atomically
    /// with the config commit.
    pub async fn replace_then<F>(
        &self,
        mut new: mando_config::Config,
        post_commit: F,
    ) -> anyhow::Result<ConfigChangeEvent>
    where
        F: FnOnce(ConfigChangeEvent),
    {
        let _guard = self.write_mu.lock().await;
        let old = (*self.config.load_full()).clone();
        new.populate_runtime_fields();
        let event = self.commit_locked(old, new).await?;
        post_commit(event);
        Ok(event)
    }

    async fn commit_locked(
        &self,
        old: mando_config::Config,
        mut new: mando_config::Config,
    ) -> anyhow::Result<ConfigChangeEvent> {
        new.populate_runtime_fields();

        let to_save = new.clone();
        tokio::task::spawn_blocking(move || mando_config::save_config(&to_save, None))
            .await
            .context("config save task panicked")??;

        sync_process_env(&old.env, &new.env);

        self.config.store(Arc::new(new.clone()));
        if self
            .tick_tx
            .send(clamped_tick_duration(new.captain.tick_interval_s))
            .is_err()
        {
            tracing::warn!(
                module = "config",
                "tick_tx has no receivers, tick interval change not propagated"
            );
        }

        let event = classify_change(&old, &new);
        if self.changes_tx.send(event).is_err() {
            tracing::warn!(
                module = "config",
                ?event,
                "changes_tx has no receivers, config change event not propagated"
            );
        }
        Ok(event)
    }
}

fn clamped_tick_duration(raw: u64) -> Duration {
    Duration::from_secs(raw.max(10))
}

fn classify_change(old: &mando_config::Config, new: &mando_config::Config) -> ConfigChangeEvent {
    let tg_changed = old.channels.telegram.enabled != new.channels.telegram.enabled
        || old.channels.telegram.owner != new.channels.telegram.owner
        || old.channels.telegram.token != new.channels.telegram.token
        || old.env.get("TELEGRAM_MANDO_BOT_TOKEN") != new.env.get("TELEGRAM_MANDO_BOT_TOKEN");

    let captain_changed = old.captain.auto_schedule != new.captain.auto_schedule
        || old.captain.tick_interval_s != new.captain.tick_interval_s;

    let ui_changed = old.ui.open_at_login != new.ui.open_at_login;

    let changed: HashSet<ConfigChangeEvent> = [
        tg_changed.then_some(ConfigChangeEvent::Telegram),
        captain_changed.then_some(ConfigChangeEvent::Captain),
        ui_changed.then_some(ConfigChangeEvent::Ui),
    ]
    .into_iter()
    .flatten()
    .collect();

    let configs_equal = match (serde_json::to_value(old), serde_json::to_value(new)) {
        (Ok(a), Ok(b)) => a == b,
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
        0 => ConfigChangeEvent::Full,
        1 => changed
            .iter()
            .copied()
            .next()
            .unwrap_or(ConfigChangeEvent::Full),
        _ => ConfigChangeEvent::Full,
    }
}

fn sync_process_env(
    old_env: &std::collections::HashMap<String, String>,
    new_env: &std::collections::HashMap<String, String>,
) {
    for key in old_env.keys() {
        if !new_env.contains_key(key) {
            // SAFETY: Mando explicitly hot-swaps env-backed integration keys at runtime.
            // We centralize all env mutation here to avoid ad hoc writes across the codebase.
            unsafe { std::env::remove_var(key) };
        }
    }

    for (key, value) in new_env {
        let changed = old_env.get(key) != Some(value);
        if changed {
            // SAFETY: Mando explicitly hot-swaps env-backed integration keys at runtime.
            // We centralize all env mutation here to avoid ad hoc writes across the codebase.
            unsafe { std::env::set_var(key, value) };
        }
    }
}

pub fn initial_tick_duration(config: &mando_config::Config) -> Duration {
    clamped_tick_duration(config.captain.tick_interval_s)
}
