//! `/stop [id]` — stop one task when an id is given, otherwise drain all workers globally.

use crate::telegram_format::escape_html;
use anyhow::Result;

use crate::bot::TelegramBot;
use crate::gateway_paths as paths;

pub async fn handle(bot: &TelegramBot, chat_id: &str, args: &str) -> Result<()> {
    let trimmed = args.trim();
    if trimmed.is_empty() {
        return stop_all(bot, chat_id).await;
    }
    match trimmed.parse::<i64>() {
        Ok(id) => stop_one(bot, chat_id, id).await,
        Err(_) => {
            bot.send_html(
                chat_id,
                &format!(
                    "❌ Invalid task id: <code>{}</code>. Usage: <code>/stop &lt;id&gt;</code> or <code>/stop</code> to drain all workers.",
                    escape_html(trimmed),
                ),
            )
            .await?;
            Ok(())
        }
    }
}

async fn stop_all(bot: &TelegramBot, chat_id: &str) -> Result<()> {
    match bot
        .gw()
        .post_no_body::<api_types::StopWorkersResponse>(paths::CAPTAIN_STOP)
        .await
    {
        Ok(resp) => {
            bot.send_html(chat_id, &format!("🛑 Stopped {} worker(s).", resp.killed))
                .await?;
        }
        Err(e) => {
            bot.send_html(
                chat_id,
                &format!("❌ Stop failed: {}", escape_html(&e.to_string())),
            )
            .await?;
        }
    }
    Ok(())
}

async fn stop_one(bot: &TelegramBot, chat_id: &str, id: i64) -> Result<()> {
    match bot
        .gw()
        .post_typed::<api_types::TaskIdRequest, api_types::BoolOkResponse>(
            paths::TASKS_STOP,
            &api_types::TaskIdRequest { id },
        )
        .await
    {
        Ok(_) => {
            bot.send_html(
                chat_id,
                &format!("🛑 Stopped task #{id}. Worktree preserved; reopen to resume."),
            )
            .await?;
        }
        Err(e) => {
            bot.send_html(
                chat_id,
                &format!(
                    "❌ Stop failed for task #{id}: {}",
                    escape_html(&e.to_string())
                ),
            )
            .await?;
        }
    }
    Ok(())
}
