//! `/answer {id} {text}` — answer clarifier questions for a task.

use crate::bot::TelegramBot;
use anyhow::Result;
use mando_shared::telegram_format::escape_html;
use serde_json::json;

/// Handle `/answer {id} {text}`.
pub async fn handle(bot: &TelegramBot, chat_id: &str, args: &str) -> Result<()> {
    let parts: Vec<&str> = args.splitn(2, char::is_whitespace).collect();

    if parts.len() < 2 || parts[0].trim().is_empty() || parts[1].trim().is_empty() {
        bot.send_html(
            chat_id,
            "Usage: /answer &lt;item_id&gt; &lt;text&gt;\n\n\
             Answers the clarifier questions for an item in NeedsClarification state.\n\n\
             Example: /answer 42 Use the new auth module instead",
        )
        .await?;
        return Ok(());
    }

    let item_id = parts[0].trim().trim_start_matches('#');
    let answer_text = parts[1].trim();

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
        .post(
            "/api/tasks/answer",
            &json!({"id": id_num, "answer": answer_text}),
        )
        .await
    {
        Ok(_) => {
            bot.send_html(
                chat_id,
                &format!(
                    "\u{2705} Answer sent for #{}\n<i>{}</i>",
                    escape_html(item_id),
                    escape_html(answer_text),
                ),
            )
            .await?;
        }
        Err(e) => {
            bot.send_html(
                chat_id,
                &format!(
                    "\u{274c} Answer failed for #{}: {}",
                    escape_html(item_id),
                    escape_html(&e.to_string()),
                ),
            )
            .await?;
        }
    }

    Ok(())
}
