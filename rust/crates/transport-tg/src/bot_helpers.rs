//! Free helper functions for the Telegram bot.

use serde_json::Value;

use crate::bot::{PickerItem, PickerState};
pub(crate) use crate::message_helpers::{bc, extract_chat_id, extract_user_id, parse_command};

pub(crate) fn to_picker_state(chat_id: &str, items: &[&api_types::TaskItem]) -> PickerState {
    PickerState {
        chat_id: chat_id.to_string(),
        items: items
            .iter()
            .map(|it| PickerItem {
                id: it.id.to_string(),
                title: it.title.clone(),
                status: Some(it.status),
                has_pr: it.pr_number.is_some(),
            })
            .collect(),
    }
}

/// Extract the highest-res photo file_id from a photo message with a `/todo` caption.
/// Returns `None` if not a photo or caption doesn't start with `/todo`.
pub(crate) fn extract_photo_todo(message: &Value) -> Option<String> {
    let photos = message.get("photo")?.as_array()?;
    let caption = message.get("caption")?.as_str()?;
    let (cmd, _) = parse_command(caption);
    if cmd != "todo" {
        return None;
    }
    photos
        .last()
        .and_then(|p| p["file_id"].as_str())
        .map(|s| s.to_string())
}

/// Build a persistent reply keyboard for DM context.
///
/// Shows common commands as quick-tap buttons at the bottom of the chat.
/// Scout button only appears when the scout feature flag is enabled.
pub(crate) fn dm_reply_keyboard(scout_enabled: bool) -> api_types::TelegramReplyMarkup {
    let mut row: Vec<String> = vec!["/tasks".into(), "/action".into(), "/todo".into()];
    if scout_enabled {
        row.push("/scout".into());
    }
    api_types::TelegramReplyMarkup::ReplyKeyboard {
        rows: vec![row],
        one_time: false,
        resize: true,
        persistent: true,
    }
}
