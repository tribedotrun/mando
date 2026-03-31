//! `/workers` — show active and stale workers.

use anyhow::Result;
use mando_shared::telegram_format::escape_html;

use crate::bot::TelegramBot;

pub async fn handle(bot: &TelegramBot, chat_id: &str, _args: &str) -> Result<()> {
    match bot.gw().get("/api/workers").await {
        Ok(resp) => {
            let workers = resp["workers"].as_array().cloned().unwrap_or_default();
            if workers.is_empty() {
                bot.send_html(chat_id, "🛌 No active workers.").await?;
                return Ok(());
            }

            let active = workers
                .iter()
                .filter(|worker| worker["is_stale"].as_bool() != Some(true))
                .count();
            let stale = workers.len().saturating_sub(active);

            let mut lines = vec![format!(
                "👷 <b>Workers</b>\n{} active · {} stale",
                active, stale
            )];

            for worker in workers {
                let id = worker["id"].as_i64().unwrap_or(0);
                let title = worker["title"].as_str().unwrap_or("Untitled");
                let project = worker["project"].as_str().unwrap_or("unknown");
                let stale_tag = if worker["is_stale"].as_bool() == Some(true) {
                    " · stale"
                } else {
                    ""
                };
                lines.push(format!(
                    "\n• <b>#{id}</b> {} <code>{}</code>{stale_tag}",
                    escape_html(title),
                    escape_html(project),
                ));
            }

            bot.send_html(chat_id, &lines.join("\n")).await?;
        }
        Err(e) => {
            bot.send_html(
                chat_id,
                &format!("❌ Failed to load workers: {}", escape_html(&e.to_string())),
            )
            .await?;
        }
    }

    Ok(())
}
