//! `/handoff [id]` — hand off a done or in-progress task to human.

use super::picker::{self, SinglePicker};
use crate::bot::TelegramBot;
use crate::gateway_paths;
use anyhow::Result;
use mando_shared::telegram_format::escape_html;
use mando_types::ItemStatus;
use serde_json::json;

const PICKER: SinglePicker = SinglePicker {
    header: "Pick a task to hand off to human:",
    empty_msg: "\u{2705} No done or in-progress tasks to hand off.",
    callback_prefix: "handoff",
    limit: 8,
    show_pr: false,
    button_text: |num| format!("\u{1f91e} #{num}"),
    store: TelegramBot::store_handoff_picker,
};

fn is_handoffable(it: &mando_types::Task) -> bool {
    matches!(
        it.status,
        ItemStatus::AwaitingReview | ItemStatus::InProgress
    )
}

/// Handle `/handoff [id]`.
pub async fn handle(bot: &mut TelegramBot, chat_id: &str, args: &str) -> Result<()> {
    let items = match super::load_tasks_or_notify(bot, chat_id).await {
        Some(items) => items,
        None => return Ok(()),
    };

    // Direct handoff by ID
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
            Some(it) if !is_handoffable(it) => {
                bot.send_html(
                    chat_id,
                    &format!(
                        "\u{26a0}\u{fe0f} Task #{} \u{2014} only done or in-progress tasks can be handed off.",
                        escape_html(target_id),
                    ),
                )
                .await?;
            }
            Some(it) => {
                let id_num = it.id;
                let title = escape_html(&it.title);
                match bot
                    .gw()
                    .post(gateway_paths::TASKS_HANDOFF, &json!({"id": id_num}))
                    .await
                {
                    Ok(_) => {
                        bot.send_html(chat_id, &format!("\u{1f91e} Handed off: {title}"))
                            .await?;
                    }
                    Err(e) => {
                        bot.send_html(
                            chat_id,
                            &format!(
                                "\u{274c} Handoff failed for #{}: {e}",
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

    // Show picker
    picker::show_single(bot, chat_id, &items, &PICKER, is_handoffable).await
}
