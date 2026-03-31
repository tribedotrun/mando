//! `/delete [id]` — permanently remove tasks.

use super::picker::{self, MultiPicker};
use crate::bot::TelegramBot;
use anyhow::Result;
use mando_shared::telegram_format::escape_html;
use serde_json::json;

const PICKER: MultiPicker = MultiPicker {
    header: "\u{26a0}\u{fe0f} Select tasks to permanently delete:",
    empty_msg: "\u{2705} Task list is empty \u{2014} nothing to delete.",
    callback_prefix: "ms_delete",
    confirm_label: "Delete Selected",
    limit: 10,
    store: TelegramBot::store_delete_picker,
};

/// Handle `/delete [id]`.
pub async fn handle(bot: &mut TelegramBot, chat_id: &str, args: &str) -> Result<()> {
    let items = match super::load_tasks_or_notify(bot, chat_id).await {
        Some(items) => items,
        None => return Ok(()),
    };

    // Direct delete by ID
    let target_id = args.trim();
    if !target_id.is_empty() {
        let target_num: Option<i64> = target_id.parse().ok();
        let item = target_num.and_then(|n| items.iter().find(|it| it.id == n));
        match item {
            None => {
                bot.send_html(
                    chat_id,
                    &format!(
                        "\u{26a0}\u{fe0f} Task #{} not found.",
                        escape_html(target_id)
                    ),
                )
                .await?;
            }
            Some(it) => {
                let id_num = it.id;
                let title = escape_html(&it.title);
                match bot
                    .gw()
                    .post("/api/tasks/delete", &json!({"ids": [id_num]}))
                    .await
                {
                    Ok(_) => {
                        bot.send_html(chat_id, &format!("\u{1f5d1}\u{fe0f} Deleted: {title}"))
                            .await?;
                    }
                    Err(e) => {
                        bot.send_html(
                            chat_id,
                            &format!(
                                "\u{274c} Delete failed for #{}: {e}",
                                escape_html(target_id)
                            ),
                        )
                        .await?;
                    }
                }
            }
        }
        return Ok(());
    }

    // Show multi-select picker (all items are deletable)
    picker::show_multi(bot, chat_id, &items, &PICKER, |_| true).await
}
