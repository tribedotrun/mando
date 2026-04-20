//! `/stop` — stop all active workers.

use crate::telegram_format::escape_html;
use anyhow::Result;

use crate::bot::TelegramBot;
use crate::gateway_paths as paths;

pub async fn handle(bot: &TelegramBot, chat_id: &str, _args: &str) -> Result<()> {
    match bot
        .gw()
        .post_no_body::<api_types::StopWorkersResponse>(paths::CAPTAIN_STOP)
        .await
    {
        Ok(resp) => {
            bot.send_html(chat_id, &format!("🛑 Stopped {} worker(s).", resp.killed))
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
