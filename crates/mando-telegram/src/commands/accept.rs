//! `/accept <id>` — accept a no-PR task.

use anyhow::Result;
use mando_shared::telegram_format::escape_html;
use serde_json::json;

use crate::bot::TelegramBot;
use crate::gateway_paths as paths;

pub async fn handle(bot: &TelegramBot, chat_id: &str, args: &str) -> Result<()> {
    let item_id = args.trim().trim_start_matches('#');
    if item_id.is_empty() {
        bot.send_html(
            chat_id,
            "Usage: /accept &lt;task_id&gt;\n\nExample: /accept 42",
        )
        .await?;
        return Ok(());
    }

    let id_num: i64 = match item_id.parse() {
        Ok(n) => n,
        Err(_) => {
            bot.send_html(
                chat_id,
                &format!("⚠️ Invalid task ID: {}", escape_html(item_id)),
            )
            .await?;
            return Ok(());
        }
    };

    match bot
        .gw()
        .post(paths::TASKS_ACCEPT, &json!({ "id": id_num }))
        .await
    {
        Ok(_) => {
            bot.send_html(chat_id, &format!("✅ Accepted #{}", escape_html(item_id)))
                .await?;
        }
        Err(e) => {
            bot.send_html(
                chat_id,
                &format!(
                    "❌ Accept failed for #{}: {}",
                    escape_html(item_id),
                    escape_html(&e.to_string())
                ),
            )
            .await?;
        }
    }

    Ok(())
}
