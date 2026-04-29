//! Polling loop and per-update spawn machinery.
//!
//! Extracted from `bot.rs` so the polling-and-spawn surface stays small and
//! the LOC cap on `bot.rs` does not block future bot work.
//!
//! Each `getUpdates` poll batches into a `JoinSet` so a slow command (e.g.
//! `/scout_research`) cannot block the next update. `try_join_next` reaps
//! completed handlers each loop iteration so panics surface and the set
//! does not grow unbounded under sustained load.

use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use serde_json::Value;
use tokio::task::JoinSet;
use tracing::{error, info, warn};

use crate::bot::TelegramBot;

/// Run the Telegram bot's polling loop.
///
/// This is the public entry point used by `lib.rs::start_bot`. It handles:
///   1. Waiting for the gateway to become reachable.
///   2. Calling `getMe` and registering the bot's `/help`-visible commands.
///   3. Loading persisted picker state from disk.
///   4. Long-polling `getUpdates` and dispatching each update on its own
///      tokio task via `JoinSet`.
pub async fn run_polling_loop(bot: Arc<TelegramBot>) -> Result<()> {
    info!("Waiting for gateway at {}", bot.gw().base_url());
    bot.gw().wait_for_gateway(Duration::from_secs(30)).await?;
    info!("Gateway reachable");

    let me = bot.api().get_me().await?;
    let username = me
        .get("username")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    info!("Telegram bot @{username} connected");
    bot.load_picker_state().await;
    bot.register_commands().await;

    let mut tasks: JoinSet<()> = JoinSet::new();
    let mut offset: i64 = 0;
    loop {
        // Reap finished handlers before each poll so panics surface and the
        // JoinSet does not grow without bound under sustained load.
        reap_completed(&mut tasks);

        let updates = match bot.api().get_updates(offset, 30).await {
            Ok(u) => u,
            Err(e) => {
                warn!("getUpdates failed: {e}");
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            }
        };
        for update in updates {
            if let Some(uid) = update.get("update_id").and_then(|v| v.as_i64()) {
                offset = uid + 1;
            }
            spawn_update(&mut tasks, Arc::clone(&bot), update);
        }
    }
}

fn reap_completed(tasks: &mut JoinSet<()>) {
    while let Some(res) = tasks.try_join_next() {
        if let Err(e) = res {
            if e.is_panic() {
                error!("telegram update handler panicked: {e}");
            } else if e.is_cancelled() {
                // Task cancellation is expected during shutdown.
            }
        }
    }
}

fn spawn_update(tasks: &mut JoinSet<()>, bot: Arc<TelegramBot>, update: Value) {
    tasks.spawn(async move {
        if let Err(e) = bot.handle_update(update).await {
            error!("Error handling update: {e}");
        }
    });
}
