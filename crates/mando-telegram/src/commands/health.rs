//! `/health` — system health + active workers.

use anyhow::Result;
use mando_shared::telegram_format::escape_html;

use crate::bot::TelegramBot;

pub async fn handle(bot: &TelegramBot, chat_id: &str, _args: &str) -> Result<()> {
    // System health
    let health_text = match bot.gw().get("/api/health/system").await {
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

            format!(
                "\u{1f9ee} <b>System Health</b>\n\
                 Daemon: v{} (pid {pid}, up {uptime_str})\n\
                 Workers: {active} active\n\
                 Tasks: {total} items\n\
                 Projects: {}",
                escape_html(version),
                escape_html(&projects),
            )
        }
        Err(e) => {
            format!(
                "\u{274c} Health check failed: {}",
                escape_html(&e.to_string())
            )
        }
    };

    // Workers
    let workers_text = match bot.gw().get("/api/workers").await {
        Ok(resp) => {
            let workers = resp["workers"].as_array().cloned().unwrap_or_default();
            if workers.is_empty() {
                "\n\n\u{1f6cc} No active workers.".to_string()
            } else {
                let mut lines = Vec::new();
                for worker in &workers {
                    let id = worker["id"].as_i64().unwrap_or(0);
                    let title = worker["title"].as_str().unwrap_or("Untitled");
                    let project = worker["project"].as_str().unwrap_or("unknown");
                    let stale_tag = if worker["is_stale"].as_bool() == Some(true) {
                        " \u{00b7} stale"
                    } else {
                        ""
                    };
                    lines.push(format!(
                        "\u{2022} <b>#{id}</b> {} <code>{}</code>{stale_tag}",
                        escape_html(title),
                        escape_html(project),
                    ));
                }
                format!("\n\n\u{1f477} <b>Workers</b>\n{}", lines.join("\n"))
            }
        }
        Err(_) => String::new(),
    };

    bot.send_html(chat_id, &format!("{health_text}{workers_text}"))
        .await?;
    Ok(())
}
