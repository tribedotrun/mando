//! `/knowledge` — list and approve pending knowledge lessons.

use crate::bot::TelegramBot;
use anyhow::Result;
use mando_shared::telegram_format::escape_html;
use serde_json::json;

/// Handle `/knowledge`.
pub async fn handle(bot: &TelegramBot, chat_id: &str, _args: &str) -> Result<()> {
    let resp = match bot.gw().get("/api/knowledge/pending").await {
        Ok(r) => r,
        Err(e) => {
            bot.send_html(
                chat_id,
                &format!(
                    "\u{274c} Failed to load knowledge: {}",
                    escape_html(&e.to_string())
                ),
            )
            .await?;
            return Ok(());
        }
    };

    let pending: Vec<serde_json::Value> = resp["pending"].as_array().cloned().unwrap_or_default();

    if pending.is_empty() {
        bot.send_html(
            chat_id,
            "\u{1f4da} <b>Knowledge</b>\n\nNo pending lessons to approve.",
        )
        .await?;
        return Ok(());
    }

    let mut lines = vec![format!(
        "\u{1f4da} <b>Pending Knowledge</b> ({} lessons)\n",
        pending.len()
    )];

    let mut buttons = Vec::new();
    for (idx, lesson) in pending.iter().take(10).enumerate() {
        let title = lesson["title"].as_str().unwrap_or("Untitled");
        let source = lesson["source"].as_str().unwrap_or("unknown");
        let id = lesson["id"].as_str().unwrap_or("");
        lines.push(format!(
            " {}. <b>{}</b> ({})",
            idx + 1,
            escape_html(title),
            escape_html(source),
        ));
        buttons.push(json!([
            {"text": format!("\u{2705} #{}", idx + 1), "callback_data": format!("knowledge:approve:{}", id)},
            {"text": format!("\u{274c} #{}", idx + 1), "callback_data": format!("knowledge:reject:{}", id)},
        ]));
    }

    bot.api()
        .send_message(
            chat_id,
            &lines.join("\n"),
            Some("HTML"),
            Some(json!({"inline_keyboard": buttons})),
            true,
        )
        .await?;

    Ok(())
}
