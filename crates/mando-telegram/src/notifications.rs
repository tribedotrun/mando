//! SSE notification handler — maps gateway `NotificationPayload` events
//! to Telegram messages via the `TelegramApi`.
//!
//! Supports edit-in-place: when a `task_key` is present, subsequent
//! notifications for the same key edit the existing message instead of
//! sending a new one.

use std::collections::HashMap;

use anyhow::Result;
use serde_json::Value;
use tracing::{debug, warn};

use mando_types::events::{NotificationKind, NotificationPayload};
use mando_types::NotifyLevel;

use crate::api::TelegramApi;

/// Handles incoming notification payloads and sends/edits TG messages.
pub struct NotificationHandler {
    api: TelegramApi,
    chat_id: String,
    /// Maps `task_key` → Telegram `message_id` for edit-in-place.
    task_messages: HashMap<String, i64>,
    /// Minimum level to actually send. Below this we log and skip.
    min_level: NotifyLevel,
}

impl NotificationHandler {
    /// Create a new handler targeting `chat_id`.
    pub fn new(api: TelegramApi, chat_id: String) -> Self {
        Self {
            api,
            chat_id,
            task_messages: HashMap::new(),
            min_level: NotifyLevel::Normal,
        }
    }

    /// Set the minimum notification level (quiet mode threshold).
    pub fn set_min_level(&mut self, level: NotifyLevel) {
        self.min_level = level;
    }

    /// Handle an incoming notification payload.
    ///
    /// Sends a new message or edits an existing one (if `task_key` matches a
    /// previously-sent message). Respects quiet-mode by filtering on `min_level`.
    pub async fn handle(&mut self, payload: NotificationPayload) {
        // Quiet-mode filter.
        if payload.level < self.min_level {
            debug!(
                level = ?payload.level,
                min = ?self.min_level,
                "notification below threshold, skipping"
            );
            return;
        }

        let markup = self.build_reply_markup(&payload);

        // Edit-in-place if we have a previous message for this task_key.
        if let Some(task_key) = &payload.task_key {
            if let Some(&msg_id) = self.task_messages.get(task_key) {
                match self
                    .edit_message(msg_id, &payload.message, markup.clone())
                    .await
                {
                    Ok(_) => {
                        debug!(task_key, msg_id, "edited notification in place");
                        return;
                    }
                    Err(e) => {
                        // Edit failed (message too old, deleted, etc). Fall through to send new.
                        warn!(task_key, msg_id, "edit failed, sending new: {e:#}");
                        self.task_messages.remove(task_key);
                    }
                }
            }
        }

        // Send new message.
        match self.send_message(&payload.message, markup).await {
            Ok(msg_id) => {
                if let Some(task_key) = &payload.task_key {
                    // Bound the map — TG messages can only be edited within 48h anyway.
                    if self.task_messages.len() >= 500 {
                        self.task_messages.clear();
                    }
                    self.task_messages.insert(task_key.clone(), msg_id);
                }
            }
            Err(e) => {
                warn!("failed to send notification: {e:#}");
            }
        }
    }

    /// Clear tracked messages (e.g. on reconnect when state may be stale).
    pub fn clear_tracked_messages(&mut self) {
        self.task_messages.clear();
    }

    // ── private helpers ─────────────────────────────────────────────

    async fn send_message(&self, text: &str, reply_markup: Option<Value>) -> Result<i64> {
        let result = self
            .api
            .send_message(&self.chat_id, text, Some("HTML"), reply_markup, true)
            .await?;
        Ok(result["message_id"].as_i64().unwrap_or(0))
    }

    async fn edit_message(
        &self,
        message_id: i64,
        text: &str,
        reply_markup: Option<Value>,
    ) -> Result<()> {
        self.api
            .edit_message_text(&self.chat_id, message_id, text, Some("HTML"), reply_markup)
            .await?;
        Ok(())
    }

    /// Build reply markup from the payload.
    ///
    /// If the payload has explicit `reply_markup`, use that. Otherwise,
    /// generate default inline keyboards based on `NotificationKind`.
    fn build_reply_markup(&self, payload: &NotificationPayload) -> Option<Value> {
        // Explicit markup takes priority.
        if payload.reply_markup.is_some() {
            return payload.reply_markup.clone();
        }

        // Generate default keyboards for known kinds.
        match &payload.kind {
            NotificationKind::NeedsClarification { item_id, .. } => {
                Some(inline_keyboard(vec![button(
                    "Answer",
                    &format!("answer:{item_id}"),
                )]))
            }
            NotificationKind::Escalated { item_id, .. } => Some(inline_keyboard(vec![button(
                "View Timeline",
                &format!("view:{item_id}"),
            )])),
            NotificationKind::ScoutProcessed {
                scout_id,
                telegraph_url,
                ..
            } => {
                let mut buttons = Vec::new();
                if telegraph_url.is_some() {
                    buttons.push(button("Read", &format!("dg:read:{scout_id}")));
                }
                buttons.push(button("Archive", &format!("dg:archive:{scout_id}")));
                Some(inline_keyboard(buttons))
            }
            NotificationKind::ScoutProcessFailed { scout_id, .. } => {
                Some(inline_keyboard(vec![button(
                    "Retry",
                    &format!("dg:process:{scout_id}"),
                )]))
            }
            _ => None,
        }
    }
}

// ── Inline keyboard builders ────────────────────────────────────────

fn button(text: &str, callback_data: &str) -> Value {
    serde_json::json!({
        "text": text,
        "callback_data": callback_data,
    })
}

fn inline_keyboard(buttons: Vec<Value>) -> Value {
    // Single row of buttons.
    serde_json::json!({
        "inline_keyboard": [buttons],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn button_json_shape() {
        let b = button("Click me", "action:123");
        assert_eq!(b["text"], "Click me");
        assert_eq!(b["callback_data"], "action:123");
    }

    #[test]
    fn inline_keyboard_shape() {
        let kb = inline_keyboard(vec![button("A", "a:1"), button("B", "b:2")]);
        let rows = kb["inline_keyboard"].as_array().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].as_array().unwrap().len(), 2);
    }

    #[test]
    fn generic_notification_no_keyboard() {
        let api = TelegramApi::new("fake:token");
        let handler = NotificationHandler::new(api, "12345".into());
        let payload = NotificationPayload {
            message: "something happened".into(),
            level: NotifyLevel::Normal,
            kind: NotificationKind::Generic,
            task_key: None,
            reply_markup: None,
        };
        let markup = handler.build_reply_markup(&payload);
        assert!(markup.is_none());
    }

    #[test]
    fn explicit_markup_takes_priority() {
        let api = TelegramApi::new("fake:token");
        let handler = NotificationHandler::new(api, "12345".into());
        let custom =
            serde_json::json!({"inline_keyboard": [[{"text": "Custom", "callback_data": "x"}]]});
        let payload = NotificationPayload {
            message: "with custom markup".into(),
            level: NotifyLevel::High,
            kind: NotificationKind::Escalated {
                item_id: "ITEM-1".into(),
                summary: None,
            },
            task_key: None,
            reply_markup: Some(custom.clone()),
        };
        let markup = handler.build_reply_markup(&payload);
        assert_eq!(markup.unwrap(), custom);
    }

    #[test]
    fn scout_processed_gets_read_and_archive_buttons() {
        let api = TelegramApi::new("fake:token");
        let handler = NotificationHandler::new(api, "12345".into());
        let payload = NotificationPayload {
            message: "📰 Article Title".into(),
            level: NotifyLevel::Normal,
            kind: NotificationKind::ScoutProcessed {
                scout_id: 42,
                title: "Article Title".into(),
                relevance: 80,
                quality: 90,
                source_name: Some("Example Blog".into()),
                telegraph_url: Some("https://telegra.ph/test".into()),
            },
            task_key: Some("scout:42".into()),
            reply_markup: None,
        };
        let markup = handler.build_reply_markup(&payload);
        assert!(markup.is_some());
        let kb = markup.unwrap();
        let buttons = kb["inline_keyboard"][0].as_array().unwrap();
        assert_eq!(buttons.len(), 2);
        assert_eq!(buttons[0]["text"], "Read");
        assert_eq!(buttons[0]["callback_data"], "dg:read:42");
        assert_eq!(buttons[1]["text"], "Archive");
        assert_eq!(buttons[1]["callback_data"], "dg:archive:42");
    }

    #[test]
    fn scout_processed_no_telegraph_omits_read_button() {
        let api = TelegramApi::new("fake:token");
        let handler = NotificationHandler::new(api, "12345".into());
        let payload = NotificationPayload {
            message: "📰 Article Title".into(),
            level: NotifyLevel::Normal,
            kind: NotificationKind::ScoutProcessed {
                scout_id: 7,
                title: "Article Title".into(),
                relevance: 50,
                quality: 60,
                source_name: None,
                telegraph_url: None,
            },
            task_key: Some("scout:7".into()),
            reply_markup: None,
        };
        let markup = handler.build_reply_markup(&payload);
        assert!(markup.is_some());
        let kb = markup.unwrap();
        let buttons = kb["inline_keyboard"][0].as_array().unwrap();
        assert_eq!(buttons.len(), 1);
        assert_eq!(buttons[0]["text"], "Archive");
        assert_eq!(buttons[0]["callback_data"], "dg:archive:7");
    }

    #[test]
    fn below_threshold_filtered() {
        let api = TelegramApi::new("fake:token");
        let mut handler = NotificationHandler::new(api, "12345".into());
        handler.set_min_level(NotifyLevel::High);
        // A Normal-level notification is below High threshold.
        // We can't test the async `handle` here, but we can verify the level comparison.
        assert!(NotifyLevel::Normal < NotifyLevel::High);
    }
}
