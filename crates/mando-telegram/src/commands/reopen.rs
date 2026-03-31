//! `/reopen` — show picker for done/failed/handed-off items to reopen.

use super::picker::{self, SinglePicker};
use crate::bot::TelegramBot;
use anyhow::Result;

const PICKER: SinglePicker = SinglePicker {
    header: "Pick a done task to reopen:",
    empty_msg: "\u{2705} No reopenable tasks.",
    callback_prefix: "reopen",
    limit: 8,
    show_pr: true,
    button_text: |num| format!("#{num}"),
    store: TelegramBot::store_reopen_picker,
};

/// Handle `/reopen`.
pub async fn handle(bot: &mut TelegramBot, chat_id: &str, _args: &str) -> Result<()> {
    let items = match super::load_tasks_or_notify(bot, chat_id).await {
        Some(items) => items,
        None => return Ok(()),
    };
    picker::show_single(bot, chat_id, &items, &PICKER, |it| {
        it.status.is_reopenable()
    })
    .await
}
