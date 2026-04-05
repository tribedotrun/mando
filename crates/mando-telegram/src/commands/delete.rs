//! `/delete [id]` — permanently remove tasks.

use super::picker::{self, DirectIdAction, MultiPicker};
use crate::bot::TelegramBot;
use anyhow::Result;
use serde_json::{json, Value};

const PICKER: MultiPicker = MultiPicker {
    header: "\u{26a0}\u{fe0f} Select tasks to permanently delete:",
    empty_msg: "\u{2705} Task list is empty \u{2014} nothing to delete.",
    callback_prefix: "ms_delete",
    confirm_label: "Delete Selected",
    limit: 10,
    store: TelegramBot::store_delete_picker,
};

fn delete_body(id: i64) -> Value {
    json!({"ids": [id]})
}

const DIRECT: DirectIdAction = DirectIdAction {
    failure_verb: "Delete",
    success_prefix: "\u{1f5d1}\u{fe0f} Deleted:",
    api_path: "/api/tasks/delete",
    build_body: delete_body,
    // Any existing task can be deleted; no eligibility guard.
    ineligibility_check: |_| None,
};

/// Handle `/delete [id]`.
pub async fn handle(bot: &mut TelegramBot, chat_id: &str, args: &str) -> Result<()> {
    let items = match super::load_tasks_or_notify(bot, chat_id).await {
        Some(items) => items,
        None => return Ok(()),
    };

    let target_id = args.trim();
    if !target_id.is_empty() {
        return picker::direct_by_id(bot, chat_id, &items, target_id, &DIRECT).await;
    }

    // Show multi-select picker (all items are deletable)
    picker::show_multi(bot, chat_id, &items, &PICKER, |_| true).await
}
