//! `/nudge <id> <message>` — nudge a stuck worker.

use anyhow::Result;
use mando_shared::telegram_format::escape_html;
use serde_json::json;

use crate::bot::TelegramBot;
use crate::gateway_paths as paths;

pub async fn handle(bot: &TelegramBot, chat_id: &str, args: &str) -> Result<()> {
    let mut parts = args.trim().splitn(2, char::is_whitespace);
    let item_id = parts.next().unwrap_or("").trim().trim_start_matches('#');
    let message = parts.next().unwrap_or("").trim();

    if item_id.is_empty() || message.is_empty() {
        bot.send_html(
            chat_id,
            "Usage: /nudge &lt;task_id&gt; &lt;message&gt;\n\nExample: /nudge 42 keep going, ship the PR",
        )
        .await?;
        return Ok(());
    }

    match bot
        .gw()
        .post(
            paths::CAPTAIN_NUDGE,
            &json!({ "item_id": item_id, "message": message }),
        )
        .await
    {
        Ok(resp) => {
            let worker = resp["worker"].as_str().unwrap_or("worker");
            bot.send_html(
                chat_id,
                &format!(
                    "📣 Nudged {} for #{}",
                    escape_html(worker),
                    escape_html(item_id)
                ),
            )
            .await?;
        }
        Err(e) => {
            bot.send_html(
                chat_id,
                &format!(
                    "❌ Nudge failed for #{}: {}",
                    escape_html(item_id),
                    escape_html(&e.to_string())
                ),
            )
            .await?;
        }
    }

    Ok(())
}
