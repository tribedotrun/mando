//! Bot API markup translator — the single sanctioned external-API boundary.
//!
//! `api_types::TelegramReplyMarkup` is a typed, tagged-union Mando shape.
//! Telegram's Bot API expects the raw `{"inline_keyboard": [[...]]}` shape.
//! This module converts between the two so the rest of the stack stays
//! typed: daemon -> api_types::TelegramReplyMarkup -> to_bot_api_json -> wire.
//!
//! Registered as the `translator` on `contracts/telegram-bot-api.toml`.
//! `serde_json::Value` is allowed **here only** because the Bot API request
//! body is schema we do not own.

use api_types::{InlineKeyboardButton, TelegramReplyMarkup};
use serde_json::{json, Value};

/// Convert our typed markup into the Bot API request-body shape.
pub fn to_bot_api_json(markup: &TelegramReplyMarkup) -> Value {
    match markup {
        TelegramReplyMarkup::InlineKeyboard { rows } => {
            let rows: Vec<Vec<Value>> = rows
                .iter()
                .map(|row| row.iter().map(inline_button_to_json).collect())
                .collect();
            json!({ "inline_keyboard": rows })
        }
        TelegramReplyMarkup::ReplyKeyboard {
            rows,
            one_time,
            resize,
            persistent,
        } => {
            // Bot API `KeyboardButton` is documented as `{"text": "..."}`.
            let keyboard: Vec<Vec<Value>> = rows
                .iter()
                .map(|row| row.iter().map(|text| json!({ "text": text })).collect())
                .collect();
            json!({
                "keyboard": keyboard,
                "one_time_keyboard": one_time,
                "resize_keyboard": resize,
                "is_persistent": persistent,
            })
        }
        TelegramReplyMarkup::ForceReply {} => {
            json!({ "force_reply": true })
        }
        TelegramReplyMarkup::RemoveKeyboard {} => {
            json!({ "remove_keyboard": true })
        }
    }
}

fn inline_button_to_json(b: &InlineKeyboardButton) -> Value {
    let mut obj = serde_json::Map::new();
    obj.insert("text".to_string(), Value::String(b.text.clone()));
    if let Some(cd) = &b.callback_data {
        obj.insert("callback_data".to_string(), Value::String(cd.clone()));
    }
    if let Some(url) = &b.url {
        obj.insert("url".to_string(), Value::String(url.clone()));
    }
    Value::Object(obj)
}
