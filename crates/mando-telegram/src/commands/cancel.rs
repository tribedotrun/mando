//! `/cancel [id]` — cancel tasks (direct ID or multi-select picker).

use super::picker::{self, MultiPicker};
use crate::bot::TelegramBot;
use anyhow::Result;
use mando_shared::telegram_format::escape_html;
use serde_json::json;

const PICKER: MultiPicker = MultiPicker {
    header: "Select items to cancel:",
    empty_msg: "\u{2705} No cancellable items.",
    callback_prefix: "ms_cancel",
    confirm_label: "Cancel Selected",
    limit: 10,
    store: TelegramBot::store_cancel_picker,
};

/// Handle `/cancel [id]`.
pub async fn handle(bot: &mut TelegramBot, chat_id: &str, args: &str) -> Result<()> {
    let items = match super::load_tasks_or_notify(bot, chat_id).await {
        Some(items) => items,
        None => return Ok(()),
    };

    // Direct cancel by ID
    let target_id = args.trim();
    if !target_id.is_empty() {
        let target_num: Option<i64> = target_id.parse().ok();
        let item = target_num.and_then(|n| items.iter().find(|it| it.id == n));
        match item {
            None => {
                bot.send_html(
                    chat_id,
                    &format!(
                        "\u{26a0}\u{fe0f} Item #{} not found.",
                        escape_html(target_id)
                    ),
                )
                .await?;
            }
            Some(it) if it.status.is_finalized() => {
                bot.send_html(
                    chat_id,
                    &format!(
                        "\u{26a0}\u{fe0f} Item #{} is already finalized.",
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
                    .post(
                        "/api/tasks/bulk",
                        &json!({"ids": [id_num], "updates": {"status": "canceled"}}),
                    )
                    .await
                {
                    Ok(_) => {
                        bot.send_html(chat_id, &format!("\u{274c} Cancelled: {title}"))
                            .await?;
                    }
                    Err(e) => {
                        bot.send_html(
                            chat_id,
                            &format!(
                                "\u{274c} Cancel failed for #{}: {e}",
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

    // Show multi-select picker
    picker::show_multi(bot, chat_id, &items, &PICKER, |it| {
        !it.status.is_finalized()
    })
    .await
}
