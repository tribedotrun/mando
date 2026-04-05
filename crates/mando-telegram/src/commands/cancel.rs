//! `/cancel [id]` — cancel tasks (direct ID or multi-select picker).

use super::picker::{self, DirectIdAction, MultiPicker};
use crate::bot::TelegramBot;
use anyhow::Result;
use serde_json::{json, Value};

const PICKER: MultiPicker = MultiPicker {
    header: "Select tasks to cancel:",
    empty_msg: "\u{2705} No cancellable tasks.",
    callback_prefix: "ms_cancel",
    confirm_label: "Cancel Selected",
    limit: 10,
    store: TelegramBot::store_cancel_picker,
};

fn cancel_body(id: i64) -> Value {
    json!({"ids": [id], "updates": {"status": "canceled"}})
}

const DIRECT: DirectIdAction = DirectIdAction {
    failure_verb: "Cancel",
    success_prefix: "\u{274c} Cancelled:",
    api_path: "/api/tasks/bulk",
    build_body: cancel_body,
    ineligibility_check: |it| {
        if it.status.is_finalized() {
            Some("is already finalized")
        } else {
            None
        }
    },
};

/// Handle `/cancel [id]`.
pub async fn handle(bot: &mut TelegramBot, chat_id: &str, args: &str) -> Result<()> {
    let items = match super::load_tasks_or_notify(bot, chat_id).await {
        Some(items) => items,
        None => return Ok(()),
    };

    let target_id = args.trim();
    if !target_id.is_empty() {
        return picker::direct_by_id(bot, chat_id, &items, target_id, &DIRECT).await;
    }

    // Show multi-select picker
    picker::show_multi(bot, chat_id, &items, &PICKER, |it| {
        !it.status.is_finalized()
    })
    .await
}
