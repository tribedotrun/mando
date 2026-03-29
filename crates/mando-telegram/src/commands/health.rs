//! `/health` — show system health (daemon, workers, config).

use anyhow::Result;

use mando_shared::telegram_format::escape_html;

use crate::bot::TelegramBot;

/// Handle `/health`.
pub async fn handle(bot: &TelegramBot, chat_id: &str, _args: &str) -> Result<()> {
    match bot.gw().get("/api/health/system").await {
        Ok(h) => {
            let version = h["version"].as_str().unwrap_or("?");
            let pid = h["pid"].as_u64().unwrap_or(0);
            let uptime = h["uptime"].as_u64().unwrap_or(0);
            let active = h["active_workers"].as_u64().unwrap_or(0);
            let total = h["total_items"].as_u64().unwrap_or(0);
            let projects = h["projects"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default();

            let uptime_str = if uptime >= 3600 {
                format!("{}h {}m", uptime / 3600, (uptime % 3600) / 60)
            } else if uptime >= 60 {
                format!("{}m {}s", uptime / 60, uptime % 60)
            } else {
                format!("{uptime}s")
            };

            let text = format!(
                "\u{1f9ee} <b>System Health</b>\n\
                 Daemon: v{} (pid {pid}, up {uptime_str})\n\
                 Workers: {active} active\n\
                 Tasks: {total} items\n\
                 Projects: {}",
                escape_html(version),
                escape_html(&projects),
            );
            bot.send_html(chat_id, &text).await?;
        }
        Err(e) => {
            bot.send_html(
                chat_id,
                &format!(
                    "\u{274c} Health check failed: {}",
                    escape_html(&e.to_string())
                ),
            )
            .await?;
        }
    }
    Ok(())
}
