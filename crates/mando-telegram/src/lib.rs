//! mando-telegram — Telegram bot for the Mando project.
//!
//! In-house implementation using raw Telegram Bot API HTTP calls.
//! No external Telegram library.
//!
//! Single unified bot (`TelegramBot`) — captain/tasks + scout commands.

pub mod api;
pub mod assistant;
pub mod bot;
mod bot_dispatch;
mod bot_helpers;
mod bot_sessions;
mod callback_actions;
pub mod callbacks;
mod callbacks_picker;
mod callbacks_session;
pub mod commands;
mod gateway_paths;
pub mod http;
mod message_helpers;
pub mod notifications;
pub mod permissions;
mod picker_store;
pub mod sse;

use std::sync::Arc;
use tokio::sync::RwLock;

use anyhow::Result;
use mando_config::settings::Config;

pub use api::{BotCommand, TelegramApi};
pub use bot::TelegramBot;

/// Resolve API base URL: env var `TG_API_BASE_URL` takes priority, then config field.
pub fn resolve_api_base_url(config_url: &Option<String>) -> Option<String> {
    std::env::var("TG_API_BASE_URL")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| config_url.clone())
        .filter(|s| !s.is_empty())
}

/// Start the main Telegram bot (blocking — runs the polling loop).
///
/// If `gw` is provided, it is used as-is (preserving CLI `--port`).
/// Otherwise falls back to `GatewayClient::discover()`.
pub async fn start_bot(config: Arc<RwLock<Config>>, gw: Option<http::GatewayClient>) -> Result<()> {
    let (token, base_url) = {
        let cfg = config.read().await;
        let tg = &cfg.channels.telegram;
        if !tg.enabled {
            tracing::info!("Telegram bot disabled in config");
            return Ok(());
        }
        if tg.token.is_empty() {
            tracing::warn!("Telegram bot token not configured");
            return Ok(());
        }
        let base_url = resolve_api_base_url(&tg.api_base_url);
        (tg.token.clone(), base_url)
    };

    if let Some(url) = &base_url {
        tracing::warn!("Telegram bot using custom API base URL: {url}");
    }
    let gw = match gw {
        Some(g) => g,
        None => http::GatewayClient::discover()?,
    };
    let mut bot = TelegramBot::with_base_url(config, &token, base_url.as_deref(), gw)?;
    bot.start().await
}
