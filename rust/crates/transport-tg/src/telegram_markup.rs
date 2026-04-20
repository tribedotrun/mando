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

#[cfg(test)]
mod tests {
    use super::*;

    /// Bot API body shape the translator must emit. Snapshot-style — if
    /// this diff fails review, the Bot API spec or the translator drifted.
    #[test]
    fn to_bot_api_json_matches_bot_api_shape() {
        let markup = TelegramReplyMarkup::InlineKeyboard {
            rows: vec![
                vec![
                    InlineKeyboardButton {
                        text: "Accept".into(),
                        callback_data: Some("accept:42".into()),
                        url: None,
                    },
                    InlineKeyboardButton {
                        text: "Docs".into(),
                        callback_data: None,
                        url: Some("https://mando.build/docs".into()),
                    },
                ],
                vec![InlineKeyboardButton {
                    text: "Reject".into(),
                    callback_data: Some("reject:42".into()),
                    url: None,
                }],
            ],
        };

        let got = to_bot_api_json(&markup);
        let expected = json!({
            "inline_keyboard": [
                [
                    { "text": "Accept", "callback_data": "accept:42" },
                    { "text": "Docs", "url": "https://mando.build/docs" },
                ],
                [
                    { "text": "Reject", "callback_data": "reject:42" }
                ]
            ]
        });
        assert_eq!(got, expected, "Bot API shape drifted");
    }

    #[test]
    fn to_bot_api_json_round_trips_through_bot_api_body_struct() {
        use serde::Deserialize;
        #[derive(Deserialize)]
        struct BotApiBody {
            inline_keyboard: Vec<Vec<BotApiButton>>,
        }
        #[derive(Deserialize)]
        struct BotApiButton {
            text: String,
            callback_data: Option<String>,
            url: Option<String>,
        }

        let markup = TelegramReplyMarkup::InlineKeyboard {
            rows: vec![vec![InlineKeyboardButton {
                text: "Go".into(),
                callback_data: Some("go".into()),
                url: None,
            }]],
        };
        let json = to_bot_api_json(&markup);
        let decoded: BotApiBody = serde_json::from_value(json).expect("decodes");
        assert_eq!(decoded.inline_keyboard.len(), 1);
        assert_eq!(decoded.inline_keyboard[0][0].text, "Go");
        assert_eq!(
            decoded.inline_keyboard[0][0].callback_data.as_deref(),
            Some("go")
        );
        assert!(decoded.inline_keyboard[0][0].url.is_none());
    }

    #[test]
    fn reply_keyboard_matches_bot_api_shape() {
        let markup = TelegramReplyMarkup::ReplyKeyboard {
            rows: vec![vec!["yes".into(), "no".into()]],
            one_time: true,
            resize: true,
            persistent: false,
        };
        let got = to_bot_api_json(&markup);
        let expected = json!({
            "keyboard": [[{ "text": "yes" }, { "text": "no" }]],
            "one_time_keyboard": true,
            "resize_keyboard": true,
            "is_persistent": false,
        });
        assert_eq!(got, expected);
    }

    #[test]
    fn reply_keyboard_round_trips_through_bot_api_body_struct() {
        use serde::Deserialize;
        #[derive(Deserialize)]
        struct BotApiBody {
            keyboard: Vec<Vec<BotApiKeyboardButton>>,
            one_time_keyboard: bool,
            resize_keyboard: bool,
            is_persistent: bool,
        }
        #[derive(Deserialize)]
        struct BotApiKeyboardButton {
            text: String,
        }

        let markup = TelegramReplyMarkup::ReplyKeyboard {
            rows: vec![vec!["go".into(), "stop".into()]],
            one_time: false,
            resize: false,
            persistent: true,
        };
        let json = to_bot_api_json(&markup);
        let decoded: BotApiBody = serde_json::from_value(json).expect("decodes");
        assert_eq!(decoded.keyboard.len(), 1);
        assert_eq!(decoded.keyboard[0].len(), 2);
        assert_eq!(decoded.keyboard[0][0].text, "go");
        assert_eq!(decoded.keyboard[0][1].text, "stop");
        assert!(!decoded.one_time_keyboard);
        assert!(!decoded.resize_keyboard);
        assert!(decoded.is_persistent);
    }

    #[test]
    fn remove_keyboard_and_force_reply_match_bot_api_shape() {
        assert_eq!(
            to_bot_api_json(&TelegramReplyMarkup::RemoveKeyboard {}),
            json!({ "remove_keyboard": true })
        );
        assert_eq!(
            to_bot_api_json(&TelegramReplyMarkup::ForceReply {}),
            json!({ "force_reply": true })
        );
    }
}
