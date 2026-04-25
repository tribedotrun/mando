//! `/health` — system health + active workers.

use crate::telegram_format::escape_html;
use anyhow::Result;

use crate::bot::TelegramBot;
use crate::gateway_paths as paths;

pub async fn handle(bot: &TelegramBot, chat_id: &str, _args: &str) -> Result<()> {
    // System health
    // Use the 5xx-tolerant variant so degraded-state responses (HTTP 503
    // with a body containing `healthy: false` + details) still render the
    // degradation info instead of failing with a raw status line.
    let health_text = match bot
        .gw()
        .get_with_body_on_5xx_typed::<api_types::SystemHealthResponse>(paths::HEALTH_SYSTEM)
        .await
    {
        Ok(h) => {
            let uptime = h.uptime;
            let projects = h.projects.join(", ");
            let uptime_str = if uptime >= 3600 {
                format!("{}h {}m", uptime / 3600, (uptime % 3600) / 60)
            } else if uptime >= 60 {
                format!("{}m {}s", uptime / 60, uptime % 60)
            } else {
                format!("{uptime}s")
            };

            format!(
                "\u{1f9ee} <b>System Health</b>\n\
                 Daemon: v{} (pid {}, up {uptime_str})\n\
                 Workers: {} active\n\
                 Tasks: {} items\n\
                 Projects: {}",
                escape_html(&h.version),
                h.pid,
                h.active_workers,
                h.total_items,
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
    let workers_text = match bot
        .gw()
        .get_typed::<api_types::WorkersResponse>(paths::WORKERS)
        .await
    {
        Ok(resp) => {
            if resp.workers.is_empty() {
                "\n\n\u{1f6cc} No active workers.".to_string()
            } else {
                let mut lines = Vec::new();
                for worker in &resp.workers {
                    let id = worker.id;
                    let title = worker.title.as_str();
                    let project = worker.project.as_str();
                    lines.push(format!(
                        "\u{2022} <b>#{id}</b> {} <code>{}</code>",
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
