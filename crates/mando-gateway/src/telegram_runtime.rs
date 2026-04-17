use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;
use tokio::sync::{Mutex, RwLock};

use transport_tg::PendingMessages;

const MAX_BACKOFF_SECS: u64 = 60;
const DEGRADED_FAILURE_COUNT: u32 = 5;
const DEGRADED_WINDOW: Duration = Duration::from_secs(300);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TelegramStatus {
    pub enabled: bool,
    pub running: bool,
    pub owner: String,
    pub last_error: Option<String>,
    pub degraded: bool,
    pub restart_count: u32,
    pub mode: &'static str,
}

#[derive(Default)]
struct TelegramRuntimeState {
    generation: u64,
    last_config: Option<settings::config::Config>,
    enabled: bool,
    running: bool,
    owner: String,
    last_error: Option<String>,
    pending: PendingMessages,
    bot_abort: Option<tokio::task::AbortHandle>,
    notification_abort: Option<tokio::task::AbortHandle>,
    failure_count: u32,
    first_failure_at: Option<Instant>,
    degraded: bool,
    restart_count: u32,
}

#[derive(Clone)]
pub struct TelegramRuntime {
    port: u16,
    auth_token: String,
    inner: Arc<Mutex<TelegramRuntimeState>>,
}

impl TelegramRuntime {
    pub fn new(port: u16, auth_token: String) -> Self {
        Self {
            port,
            auth_token,
            inner: Arc::new(Mutex::new(TelegramRuntimeState::default())),
        }
    }

    pub async fn configure(&self, config: &settings::config::Config) -> anyhow::Result<()> {
        let enabled = telegram_enabled(config);
        let owner = config.channels.telegram.owner.clone();

        let (generation, token, cfg_clone, start_notification) = {
            let mut state = self.inner.lock().await;
            state.generation += 1;
            abort_locked(&mut state);
            state.last_config = Some(config.clone());
            state.enabled = enabled;
            state.owner = owner.clone();
            state.last_error = None;
            state.running = false;
            state.failure_count = 0;
            state.first_failure_at = None;
            state.degraded = false;

            if !enabled {
                return Ok(());
            }

            state.running = true;
            (
                state.generation,
                config.channels.telegram.token.clone(),
                config.clone(),
                !owner.is_empty(),
            )
        };

        let gw = transport_tg::http::GatewayClient::new(self.port, Some(self.auth_token.clone()));
        let cfg = Arc::new(RwLock::new(cfg_clone));
        let pending = {
            let state = self.inner.lock().await;
            state.pending.clone()
        };

        let runtime = self.clone();
        let bot_handle = tokio::spawn(async move {
            match transport_tg::start_bot(cfg, Some(gw), pending).await {
                Ok(()) => {
                    runtime.handle_task_exit(generation, "telegram bot exited cleanly".to_string());
                }
                Err(err) => {
                    runtime.handle_task_exit(generation, format!("telegram bot stopped: {err}"));
                }
            }
        });
        {
            let mut state = self.inner.lock().await;
            if state.generation == generation {
                state.bot_abort = Some(bot_handle.abort_handle());
            }
        }

        if start_notification {
            self.spawn_notification_loop(generation, &token, &owner)
                .await?;
        }

        Ok(())
    }

    /// Abort all TG tasks during daemon shutdown.
    pub async fn shutdown(&self) {
        let mut state = self.inner.lock().await;
        abort_locked(&mut state);
        state.running = false;
        tracing::info!(module = "telegram", "telegram runtime shut down");
    }

    pub async fn restart(&self) -> anyhow::Result<()> {
        let cfg = {
            let state = self.inner.lock().await;
            state.last_config.clone()
        };
        if let Some(cfg) = cfg {
            self.configure(&cfg).await
        } else {
            Ok(())
        }
    }

    pub async fn register_owner(&self, owner: String) -> anyhow::Result<()> {
        let token = {
            let mut state = self.inner.lock().await;
            state.owner = owner.clone();
            if let Some(cfg) = state.last_config.as_mut() {
                cfg.channels.telegram.owner = owner.clone();
                cfg.channels.telegram.enabled = telegram_enabled(cfg);
                cfg.channels.telegram.token = cfg
                    .env
                    .get("TELEGRAM_MANDO_BOT_TOKEN")
                    .cloned()
                    .unwrap_or_default();
            }
            if !state.enabled || state.notification_abort.is_some() {
                return Ok(());
            }
            state
                .last_config
                .as_ref()
                .map(|cfg| cfg.channels.telegram.token.clone())
                .unwrap_or_default()
        };

        if token.is_empty() || owner.is_empty() {
            return Ok(());
        }

        let generation = {
            let state = self.inner.lock().await;
            state.generation
        };
        self.spawn_notification_loop(generation, &token, &owner)
            .await
    }

    pub async fn status(&self) -> TelegramStatus {
        let state = self.inner.lock().await;
        TelegramStatus {
            enabled: state.enabled,
            running: state.running,
            owner: state.owner.clone(),
            last_error: state.last_error.clone(),
            degraded: state.degraded,
            restart_count: state.restart_count,
            mode: "embedded",
        }
    }

    async fn spawn_notification_loop(
        &self,
        generation: u64,
        token: &str,
        owner: &str,
    ) -> anyhow::Result<()> {
        let api_base_url = transport_tg::resolve_api_base_url();
        let api = match &api_base_url {
            Some(url) => transport_tg::TelegramApi::with_base_url(token, url)?,
            None => transport_tg::TelegramApi::new(token),
        };
        let runtime = self.clone();
        let base_url = format!("http://127.0.0.1:{}", self.port);
        let gw_token = Some(self.auth_token.clone());
        let gw = transport_tg::http::GatewayClient::new(self.port, Some(self.auth_token.clone()));
        let pending = {
            let state = self.inner.lock().await;
            state.pending.clone()
        };
        let owner_chat_id = owner.to_string();
        let notif_handle = tokio::spawn(async move {
            transport_tg::sse::run_notification_loop(
                base_url,
                gw_token,
                api,
                owner_chat_id,
                gw,
                pending,
            )
            .await;
            runtime.handle_task_exit(generation, "telegram notifications stopped".to_string());
        });

        let mut state = self.inner.lock().await;
        if state.generation == generation {
            state.notification_abort = Some(notif_handle.abort_handle());
        }
        Ok(())
    }

    fn handle_task_exit(&self, generation: u64, message: String) {
        let runtime = self.clone();
        tokio::spawn(async move {
            let (cfg, backoff, post_bump_gen) = {
                let mut state = runtime.inner.lock().await;
                if state.generation != generation {
                    return;
                }
                state.running = false;
                state.last_error = Some(message.clone());
                abort_locked(&mut state);
                // Bump generation so the *other* task's handler (bot vs notification)
                // sees a mismatch and returns early -- prevents double failure counting.
                state.generation += 1;
                let post_bump_gen = state.generation;

                let now = Instant::now();
                state.failure_count += 1;
                if state.first_failure_at.is_none() {
                    state.first_failure_at = Some(now);
                }

                // Degraded: N failures within the window = stop retrying.
                if state.failure_count >= DEGRADED_FAILURE_COUNT {
                    if let Some(first) = state.first_failure_at {
                        if now.duration_since(first) < DEGRADED_WINDOW {
                            state.degraded = true;
                            tracing::error!(
                                module = "telegram",
                                failures = state.failure_count,
                                "telegram entered degraded mode after {} failures -- manual restart or config change required",
                                state.failure_count
                            );
                            return;
                        }
                    }
                    // Outside the window -- reset the counter.
                    state.failure_count = 1;
                    state.first_failure_at = Some(now);
                }

                let backoff_secs = (1u64 << (state.failure_count - 1).min(6)).min(MAX_BACKOFF_SECS);

                tracing::warn!(
                    module = "telegram",
                    failure_count = state.failure_count,
                    backoff_secs,
                    "{message} -- restarting in {backoff_secs}s"
                );

                let cfg = state.last_config.clone().filter(telegram_enabled);
                (cfg, Duration::from_secs(backoff_secs), post_bump_gen)
            };

            if cfg.is_some() {
                tokio::time::sleep(backoff).await;

                // Re-check generation after backoff: if configure() was called
                // while we slept (user changed TG settings), a newer config is
                // already running -- our captured cfg is stale, so bail out.
                let stale = {
                    let state = runtime.inner.lock().await;
                    state.generation != post_bump_gen
                };
                if stale {
                    return;
                }

                {
                    let mut state = runtime.inner.lock().await;
                    state.restart_count += 1;
                }

                // Re-read config from state so we use the latest version.
                let fresh_cfg = {
                    let state = runtime.inner.lock().await;
                    state.last_config.clone().filter(telegram_enabled)
                };
                if let Some(cfg) = fresh_cfg {
                    if let Err(err) = runtime.configure(&cfg).await {
                        let mut state = runtime.inner.lock().await;
                        state.last_error = Some(format!("telegram restart failed: {err}"));
                    }
                }
            }
        });
    }
}

fn abort_locked(state: &mut TelegramRuntimeState) {
    if let Some(handle) = state.bot_abort.take() {
        handle.abort();
    }
    if let Some(handle) = state.notification_abort.take() {
        handle.abort();
    }
}

fn telegram_enabled(config: &settings::config::Config) -> bool {
    config.channels.telegram.enabled && !config.channels.telegram.token.is_empty()
}
