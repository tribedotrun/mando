use serde_json::Value;

use crate::api::BotCommand;

pub(crate) fn parse_command(text: &str) -> (String, &str) {
    let text = text.trim();
    let without_slash = &text[1..];
    if let Some(idx) = without_slash.find(char::is_whitespace) {
        let cmd = without_slash[..idx]
            .split('@')
            .next()
            .unwrap_or(&without_slash[..idx]);
        (cmd.to_lowercase(), without_slash[idx..].trim())
    } else {
        let cmd = without_slash.split('@').next().unwrap_or(without_slash);
        (cmd.to_lowercase(), "")
    }
}

pub(crate) fn extract_chat_id(msg: &Value) -> String {
    msg.get("chat")
        .and_then(|c| c.get("id"))
        .and_then(|v| v.as_i64())
        .map(|id| id.to_string())
        .unwrap_or_default()
}

pub(crate) fn extract_user_id(msg: &Value) -> String {
    let from = msg.get("from");
    let numeric = from
        .and_then(|f| f.get("id"))
        .and_then(|v| v.as_i64())
        .map(|id| id.to_string())
        .unwrap_or_default();
    let username = from
        .and_then(|f| f.get("username"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if username.is_empty() {
        numeric
    } else {
        format!("{numeric}|{username}")
    }
}

pub(crate) fn is_group_chat(msg: &Value) -> bool {
    msg.get("chat")
        .and_then(|c| c.get("type"))
        .and_then(|t| t.as_str())
        .map(|t| t == "group" || t == "supergroup")
        .unwrap_or(false)
}

pub(crate) fn bc(command: &str, description: &str) -> BotCommand {
    BotCommand {
        command: command.into(),
        description: description.into(),
    }
}
