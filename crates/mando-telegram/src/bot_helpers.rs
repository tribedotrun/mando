//! Free helper functions for the Telegram bot.

use serde_json::Value;

use crate::bot::{PickerItem, PickerState};
pub(crate) use crate::message_helpers::{bc, extract_chat_id, extract_user_id, parse_command};

pub(crate) fn to_picker_state(chat_id: &str, items: &[&mando_types::Task]) -> PickerState {
    PickerState {
        chat_id: chat_id.to_string(),
        items: items
            .iter()
            .map(|it| PickerItem {
                id: it.id.to_string(),
                title: it.title.clone(),
                status: Some(
                    serde_json::to_value(it.status)
                        .ok()
                        .and_then(|v| v.as_str().map(String::from))
                        .unwrap_or_else(|| format!("{:?}", it.status)),
                ),
                has_pr: it.pr_number.is_some(),
            })
            .collect(),
        selected: std::collections::HashSet::new(),
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
pub(crate) fn dm_reply_keyboard(scout_enabled: bool) -> Value {
    let mut row = vec!["/tasks", "/action", "/todo"];
    if scout_enabled {
        row.push("/scout");
    }
    serde_json::json!({
        "keyboard": [row],
        "resize_keyboard": true,
        "is_persistent": true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_command_simple() {
        let (cmd, args) = parse_command("/tasks all");
        assert_eq!(cmd, "tasks");
        assert_eq!(args, "all");
    }

    #[test]
    fn parse_command_no_args() {
        let (cmd, args) = parse_command("/help");
        assert_eq!(cmd, "help");
        assert_eq!(args, "");
    }

    #[test]
    fn parse_command_with_bot_mention() {
        let (cmd, args) = parse_command("/tasks@mando_bot all");
        assert_eq!(cmd, "tasks");
        assert_eq!(args, "all");
    }

    #[test]
    fn parse_command_multiline_args() {
        let (cmd, args) = parse_command("/todo item one\nitem two");
        assert_eq!(cmd, "todo");
        assert_eq!(args, "item one\nitem two");
    }

    #[test]
    fn parse_command_uppercase() {
        let (cmd, _) = parse_command("/TASKS");
        assert_eq!(cmd, "tasks");
    }

    #[test]
    fn extract_chat_id_works() {
        let msg = serde_json::json!({"chat": {"id": 12345, "type": "private"}});
        assert_eq!(extract_chat_id(&msg), "12345");
    }

    #[test]
    fn extract_user_id_includes_username() {
        let msg = serde_json::json!({"from": {"id": 12345, "username": "bob"}});
        assert_eq!(extract_user_id(&msg), "12345|bob");
    }

    #[test]
    fn extract_user_id_numeric_only_when_no_username() {
        let msg = serde_json::json!({"from": {"id": 12345}});
        assert_eq!(extract_user_id(&msg), "12345");
    }
}
