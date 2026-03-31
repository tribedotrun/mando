//! `/history [id]` — Q&A history for a task.

use crate::bot::TelegramBot;
use anyhow::Result;
use mando_shared::telegram_format::escape_html;

/// Handle `/history [id]`.
pub async fn handle(bot: &TelegramBot, chat_id: &str, args: &str) -> Result<()> {
    let item_id = args.trim();
    if item_id.is_empty() {
        bot.send_html(chat_id, "Usage: /history &lt;task_id&gt;")
            .await?;
        return Ok(());
    }

    let resp = match bot
        .gw()
        .get(&format!("/api/tasks/{}/history", item_id))
        .await
    {
        Ok(r) => r,
        Err(e) => {
            bot.send_html(
                chat_id,
                &format!(
                    "\u{274c} Failed to load history: {}",
                    escape_html(&e.to_string())
                ),
            )
            .await?;
            return Ok(());
        }
    };

    let entries = resp["history"].as_array().cloned().unwrap_or_default();

    if entries.is_empty() {
        bot.send_html(
            chat_id,
            &format!(
                "\u{1f4dc} <b>History for #{}</b>\n\nNo Q&A history found.",
                escape_html(item_id)
            ),
        )
        .await?;
        return Ok(());
    }

    let mut lines = vec![format!(
        "\u{1f4dc} <b>History for #{}</b>\n",
        escape_html(item_id)
    )];

    for entry in entries.iter().take(10) {
        let role = entry["role"].as_str().unwrap_or("?");
        let text = entry["text"].as_str().unwrap_or("");
        let ts = entry["ts"].as_str().unwrap_or("");
        let short_ts = &ts[..ts.floor_char_boundary(16)];
        let icon = if role == "human" {
            "\u{1f464}"
        } else {
            "\u{1f916}"
        };

        let truncated = &text[..text.floor_char_boundary(200)];
        lines.push(format!(
            "{} <code>{}</code>\n{}",
            icon,
            escape_html(short_ts),
            escape_html(truncated),
        ));
    }

    if entries.len() > 10 {
        lines.push(format!(
            "\n\u{2026} and {} more entries",
            entries.len() - 10
        ));
    }

    bot.send_html(chat_id, &lines.join("\n")).await?;
    Ok(())
}
