//! `/stop` — stop all active workers.

use anyhow::Result;
use mando_shared::telegram_format::escape_html;
use serde_json::json;

use crate::bot::TelegramBot;
use crate::gateway_paths as paths;

pub async fn handle(bot: &TelegramBot, chat_id: &str, _args: &str) -> Result<()> {
    match bot.gw().post(paths::CAPTAIN_STOP, &json!({})).await {
        Ok(resp) => {
            let killed = resp["killed"].as_u64().unwrap_or(0);
            bot.send_html(chat_id, &format!("🛑 Stopped {} worker(s).", killed))
                .await?;
        }
        Err(e) => {
            bot.send_html(
                chat_id,
                &format!("❌ Stop failed: {}", escape_html(&e.to_string())),
            )
            .await?;
        }
    }

    Ok(())
}
