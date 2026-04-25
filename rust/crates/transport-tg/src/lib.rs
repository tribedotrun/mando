//! mando-telegram — Telegram bot for the Mando project.
//!
//! In-house implementation using raw Telegram Bot API HTTP calls.
//! No external Telegram library.
//!
//! Single unified bot (`TelegramBot`) — captain/tasks + scout commands.

mod config;
mod io;
mod runtime;
mod service;
mod types;

mod api;
mod api_error;
mod assistant;
mod bot;
mod bot_dispatch;
mod bot_helpers;
mod bot_sessions;
mod callback_actions;
mod callbacks;
mod callbacks_picker;
mod commands;
mod gateway_paths;
mod http;
mod message_helpers;
mod notifications;
mod permissions;
mod picker_store;
mod sse;
mod telegram_format;
mod telegram_markup;
mod telegram_tables;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::RwLock;

use anyhow::Result;
use settings::Config;

pub use api::{BotCommand, TelegramApi};
pub use bot::TelegramBot;
pub use runtime::TelegramRuntime;
pub use types::TelegramStatus;

/// Shared map of `task_key → message_id` for scout "processing..." messages.
///
/// When `add_and_track` sends a "processing..." message, it registers the
/// message_id here so the SSE notification handler can edit it in-place
/// with the full summary card (instead of creating a duplicate message).
pub type PendingMessages = Arc<Mutex<HashMap<String, i64>>>;

/// Override Telegram API base URL via `TG_API_BASE_URL` env var (dev/test only).
pub fn resolve_api_base_url() -> Option<String> {
    std::env::var("TG_API_BASE_URL")
        .ok()
        .filter(|s| !s.is_empty())
}

/// Start the main Telegram bot (blocking — runs the polling loop).
///
/// If `gw` is provided, it is used as-is (preserving CLI `--port`).
/// Otherwise falls back to `GatewayClient::discover()`.
pub async fn start_bot(
    config: Arc<RwLock<Config>>,
    gw: Option<http::GatewayClient>,
    pending: PendingMessages,
) -> Result<()> {
    let (token, base_url) = {
        let cfg = config.read().await;
        let tg = &cfg.channels.telegram;
        if !tg.enabled {
            tracing::info!(
                module = "transport-tg-lib",
                "Telegram bot disabled in config"
            );
            return Ok(());
        }
        if tg.token.is_empty() {
            tracing::warn!(
                module = "transport-tg-lib",
                "Telegram bot token not configured"
            );
            return Ok(());
        }
        let base_url = resolve_api_base_url();
        (tg.token.clone(), base_url)
    };

    if let Some(url) = &base_url {
        tracing::info!(
            module = "transport-tg-lib",
            "Telegram bot using custom API base URL: {url}"
        );
    }
    let gw = match gw {
        Some(g) => g,
        None => http::GatewayClient::discover()?,
    };
    let mut bot = TelegramBot::with_base_url(config, &token, base_url.as_deref(), gw, pending)?;
    bot.start().await
}
