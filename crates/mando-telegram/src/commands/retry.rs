//! `/retry {id}` — retry an errored captain review for a task.

use crate::bot::TelegramBot;
use anyhow::Result;
use mando_shared::telegram_format::escape_html;
use serde_json::json;

/// Handle `/retry {id}`.
pub async fn handle(bot: &TelegramBot, chat_id: &str, args: &str) -> Result<()> {
    let item_id = args.trim().trim_start_matches('#');

    if item_id.is_empty() {
        bot.send_html(
            chat_id,
            "Usage: /retry &lt;item_id&gt;\n\n\
             Retries captain review for an item in Errored state.\n\n\
             Example: /retry 42",
        )
        .await?;
        return Ok(());
    }

    let id_num: i64 = match item_id.parse() {
        Ok(n) => n,
        Err(_) => {
            bot.send_html(
                chat_id,
                &format!("\u{26a0}\u{fe0f} Invalid item ID: {}", escape_html(item_id)),
            )
            .await?;
            return Ok(());
        }
    };

    match bot
        .gw()
        .post("/api/tasks/retry", &json!({"id": id_num}))
        .await
    {
        Ok(_) => {
            bot.send_html(
                chat_id,
                &format!("\u{2705} Retry queued for #{}", escape_html(item_id),),
            )
            .await?;
        }
        Err(e) => {
            bot.send_html(
                chat_id,
                &format!(
                    "\u{274c} Retry failed for #{}: {}",
                    escape_html(item_id),
                    escape_html(&e.to_string()),
                ),
            )
            .await?;
        }
    }

    Ok(())
}
