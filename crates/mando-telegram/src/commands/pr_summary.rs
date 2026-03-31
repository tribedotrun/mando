//! `/prsummary <task_id>` — display PR description for a task.

use crate::bot::TelegramBot;
use anyhow::Result;
use mando_shared::telegram_format::escape_html;

/// Handle `/prsummary <task_id>`.
pub async fn handle(bot: &TelegramBot, chat_id: &str, args: &str) -> Result<()> {
    let item_id = args.trim();
    if item_id.is_empty() {
        bot.send_html(chat_id, "Usage: /prsummary &lt;task_id&gt;")
            .await?;
        return Ok(());
    }

    let items = match super::load_tasks_or_notify(bot, chat_id).await {
        Some(items) => items,
        None => return Ok(()),
    };
    let id_num: Option<i64> = item_id.parse().ok();
    let item = id_num.and_then(|n| items.iter().find(|it| it.id == n));

    match item {
        None => {
            bot.send_html(
                chat_id,
                &format!("\u{26a0}\u{fe0f} Task #{} not found.", escape_html(item_id)),
            )
            .await?;
        }
        Some(it) => {
            let pr_info = it
                .pr
                .as_deref()
                .map(|url| {
                    format!(
                        "\u{1f517} <a href=\"{}\">{}</a>",
                        escape_html(url),
                        escape_html(url)
                    )
                })
                .unwrap_or_else(|| "No PR associated.".to_string());
            let title = escape_html(&it.title);
            bot.send_html(
                chat_id,
                &format!("<b>#{} {}</b>\n\n{}", escape_html(item_id), title, pr_info),
            )
            .await?;
        }
    }
    Ok(())
}
