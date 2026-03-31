//! `/rework` — tear down current worker + PR and start fresh.

use super::picker::{self, SinglePicker};
use crate::bot::TelegramBot;
use anyhow::Result;

const PICKER: SinglePicker = SinglePicker {
    header:
        "\u{26a0}\u{fe0f} Rework tears down current worker + PR and starts fresh.\nPick a task:",
    empty_msg: "\u{2705} No tasks to rework.",
    callback_prefix: "rework",
    limit: 8,
    show_pr: true,
    button_text: |num| format!("#{num} \u{1f504}"),
    store: TelegramBot::store_rework_picker,
};

/// Handle `/rework`.
pub async fn handle(bot: &mut TelegramBot, chat_id: &str, _args: &str) -> Result<()> {
    let items = match super::load_tasks_or_notify(bot, chat_id).await {
        Some(items) => items,
        None => return Ok(()),
    };
    picker::show_single(bot, chat_id, &items, &PICKER, |it| {
        it.status.is_reworkable()
    })
    .await
}
