//! mando-tg — standalone Telegram bot binary.
//!
//! Connects to the mando-gw daemon over HTTP/SSE and runs the unified bot
//! and SSE notification listener as concurrent tokio tasks.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use clap::Parser;
use tokio::sync::RwLock;
use tracing::info;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

use mando_telegram::api::TelegramApi;
use mando_telegram::http::GatewayClient;
use mando_telegram::notifications::NotificationHandler;
use mando_telegram::resolve_api_base_url;
use mando_telegram::sse::{SseConsumer, SseEvent};

#[derive(Parser)]
#[command(name = "mando-tg", about = "Mando Telegram bot (standalone)")]
struct Args {
    /// Gateway port (reads daemon.port if omitted)
    #[arg(short = 'p', long)]
    port: Option<u16>,

    /// Override data directory (sets MANDO_DATA_DIR)
    #[arg(long)]
    data_dir: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // Tracing: stderr (human-readable) + JSONL file (structured, tailed by Vector).
    init_tracing();

    // Override data dir if requested (before any config access).
    if let Some(ref dir) = args.data_dir {
        // SAFETY: called before any spawns.
        unsafe { std::env::set_var("MANDO_DATA_DIR", dir) };
    }

    // Load config.
    let config = mando_config::load_config(None);

    // Inject env vars from config into process environment.
    for (k, v) in &config.env {
        // SAFETY: single-threaded at this point, before any spawns.
        unsafe { std::env::set_var(k, v) };
    }

    // Create gateway client.
    let gw = match args.port {
        Some(port) => {
            let token = read_auth_token();
            GatewayClient::new(port, token)
        }
        None => GatewayClient::discover()?,
    };

    // Wait for gateway to be reachable.
    info!("Waiting for gateway at {}", gw.base_url());
    gw.wait_for_gateway(Duration::from_secs(60)).await?;
    info!("Gateway reachable");

    let mut set = tokio::task::JoinSet::new();

    let tg = &config.channels.telegram;
    let tg_enabled = tg.enabled && !tg.token.is_empty();

    // Spawn SSE notification listener — sends alerts to telegram.owner.
    // Skipped when no owner is configured — may be auto-registered later via /start.
    if tg_enabled {
        if tg.owner.is_empty() {
            info!("No notification target configured — SSE listener skipped until owner registers via /start");
        } else {
            let base_url = gw.base_url().to_string();
            let gw_token = gw.token().map(String::from);
            let api_base_url = resolve_api_base_url();
            let api = match &api_base_url {
                Some(url) => TelegramApi::with_base_url(&tg.token, url)?,
                None => TelegramApi::new(&tg.token),
            };
            let chat_id = tg.owner.clone();
            set.spawn(async move {
                run_notification_listener(base_url, gw_token, api, chat_id).await;
            });
        }
    }

    // Spawn main bot.
    if tg_enabled {
        let cfg = Arc::new(RwLock::new(config.clone()));
        let bot_gw = gw.clone();
        set.spawn(async move {
            if let Err(e) = mando_telegram::start_bot(cfg, Some(bot_gw)).await {
                tracing::error!("[telegram] bot stopped: {e}");
            }
        });
    }

    // Spawn gateway health watchdog — exits the process when gateway is confirmed dead.
    {
        let watchdog_gw = gw.clone();
        set.spawn(async move {
            run_gateway_watchdog(watchdog_gw).await;
        });
    }

    if set.len() == 1 {
        // Only the watchdog — no bots enabled.
        tracing::warn!("No Telegram bots enabled in config, exiting");
        return Ok(());
    }

    info!("mando-tg running ({} tasks)", set.len());

    // Wait for any task to complete — all tasks run forever, so any exit is abnormal.
    match set.join_next().await {
        Some(Err(e)) => tracing::error!("task panicked: {e}"),
        Some(Ok(())) => tracing::warn!("task exited (gateway watchdog or bot stopped)"),
        None => {}
    }

    // Non-zero exit so supervisors know to restart.
    std::process::exit(1)
}

/// Two-layer tracing subscriber: stderr (human) + JSONL file (Vector).
fn init_tracing() {
    let make_filter = || {
        let base = std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into());
        EnvFilter::new(format!(
            "{base},h2=off,hyper_util=off,reqwest=warn,tonic=warn,tower=warn"
        ))
    };

    // Layer 1: human-readable stderr
    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_target(true)
        .with_filter(make_filter());

    // Layer 2: structured JSONL to {data_dir}/logs/tg-bot.jsonl
    let data_dir = mando_config::data_dir();
    let json_dir = data_dir.join("logs");
    if let Err(e) = std::fs::create_dir_all(&json_dir) {
        eprintln!(
            "FATAL: cannot create log directory {}: {e}",
            json_dir.display()
        );
        std::process::exit(1);
    }
    let json_appender = tracing_appender::rolling::daily(&json_dir, "tg-bot.jsonl");
    let json_layer = tracing_subscriber::fmt::layer()
        .json()
        .with_writer(json_appender)
        .with_target(true)
        .with_current_span(true)
        .with_span_list(true)
        .with_filter(make_filter());

    tracing_subscriber::registry()
        .with(fmt_layer)
        .with(json_layer)
        .init();
}

/// Gateway health watchdog — polls `/api/health` and exits when the gateway is
/// confirmed dead (5 consecutive failures ≈ 2.5 minutes with 30s interval).
async fn run_gateway_watchdog(gw: GatewayClient) {
    const INTERVAL: Duration = Duration::from_secs(30);
    const MAX_FAILURES: u32 = 5;

    let mut consecutive_failures: u32 = 0;

    loop {
        tokio::time::sleep(INTERVAL).await;

        match gw.health().await {
            Ok(_) => {
                if consecutive_failures > 0 {
                    info!("gateway health recovered after {consecutive_failures} failure(s)");
                }
                consecutive_failures = 0;
            }
            Err(e) => {
                consecutive_failures += 1;
                tracing::warn!(
                    consecutive = consecutive_failures,
                    max = MAX_FAILURES,
                    "gateway health check failed: {e:#}"
                );
                if consecutive_failures >= MAX_FAILURES {
                    tracing::error!(
                        "gateway unreachable for {consecutive_failures} consecutive checks, exiting"
                    );
                    return;
                }
            }
        }
    }
}

/// SSE notification loop — reconnects on failure.
async fn run_notification_listener(
    base_url: String,
    token: Option<String>,
    api: TelegramApi,
    chat_id: String,
) {
    let sse = SseConsumer::new(&base_url, token);
    let mut handler = NotificationHandler::new(api, chat_id);

    // SseConsumer handles reconnection internally — subscribe once and consume.
    let mut rx = match sse.subscribe().await {
        Ok(rx) => rx,
        Err(e) => {
            tracing::error!("SSE subscribe failed: {e}");
            return;
        }
    };

    while let Some(event) = rx.recv().await {
        match event {
            SseEvent::Notification(payload) => {
                handler.handle(payload).await;
            }
            SseEvent::Reconnected => {
                handler.clear_tracked_messages();
            }
            _ => {}
        }
    }
    tracing::warn!("SSE notification listener exited");
}

/// Read auth token from data dir.
fn read_auth_token() -> Option<String> {
    let path = mando_config::data_dir().join("auth-token");
    match std::fs::read_to_string(&path) {
        Ok(s) => {
            let trimmed = s.trim().to_string();
            if trimmed.is_empty() {
                tracing::warn!("auth-token file is empty: {}", path.display());
                None
            } else {
                Some(trimmed)
            }
        }
        Err(e) => {
            tracing::warn!("could not read auth-token from {}: {e}", path.display());
            None
        }
    }
}
