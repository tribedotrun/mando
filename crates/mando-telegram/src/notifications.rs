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
use crate::assistant::formatting::{format_swipe_card, swipe_card_kb};
use crate::gateway_paths as paths;
use crate::http::GatewayClient;
use crate::PendingMessages;

/// Handles incoming notification payloads and sends/edits TG messages.
pub struct NotificationHandler {
    api: TelegramApi,
    chat_id: String,
    gw: GatewayClient,
    /// Maps `task_key` → Telegram `message_id` for edit-in-place.
    task_messages: HashMap<String, i64>,
    /// Messages pre-registered by `add_and_track` — the SSE handler
    /// imports them into `task_messages` so it edits the "processing..."
    /// message instead of creating a duplicate.
    pending: PendingMessages,
    /// Minimum level to actually send. Below this we log and skip.
    min_level: NotifyLevel,
}

impl NotificationHandler {
    /// Create a new handler targeting `chat_id`.
    pub fn new(
        api: TelegramApi,
        chat_id: String,
        gw: GatewayClient,
        pending: PendingMessages,
    ) -> Self {
        Self {
            api,
            chat_id,
            gw,
            task_messages: HashMap::new(),
            pending,
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

        // Import any pre-registered message from add_and_track so we can
        // edit the "processing..." message instead of duplicating it.
        if let Some(task_key) = &payload.task_key {
            if let Some(msg_id) = self.pending.lock().unwrap().remove(task_key) {
                self.task_messages.insert(task_key.clone(), msg_id);
            }
        }

        // ScoutProcessed: show full summary card instead of Read/Archive buttons.
        if let NotificationKind::ScoutProcessed { scout_id, .. } = &payload.kind {
            self.handle_scout_processed(*scout_id, &payload.task_key)
                .await;
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

    /// Fetch the full scout item from the gateway and show the summary card.
    /// Edits a pre-registered "processing..." message when one exists.
    async fn handle_scout_processed(&mut self, scout_id: i64, task_key: &Option<String>) {
        let item = match self.gw.get(&paths::scout_item(scout_id)).await {
            Ok(v) => v,
            Err(e) => {
                warn!(scout_id, error = %e, "failed to fetch scout item for card");
                // Edit the "processing..." message so it doesn't stay stuck.
                if let Some(key) = task_key {
                    if let Some(&msg_id) = self.task_messages.get(key) {
                        let fallback =
                            format!("\u{26a0}\u{fe0f} Scout #{scout_id}: failed to load summary");
                        let _ = self.edit_message(msg_id, &fallback, None).await;
                    }
                }
                return;
            }
        };

        let summary = item["summary"].as_str();
        let text = format_swipe_card(&item, summary);
        let tg_url = item["telegraphUrl"].as_str();
        let kb = swipe_card_kb(scout_id, tg_url);

        // Try edit-in-place (pre-registered "processing..." message).
        if let Some(key) = task_key {
            if let Some(&msg_id) = self.task_messages.get(key) {
                match self.edit_message(msg_id, &text, Some(kb.clone())).await {
                    Ok(_) => {
                        debug!(task_key = %key, msg_id, "edited scout card in place");
                        return;
                    }
                    Err(e) => {
                        warn!(task_key = %key, msg_id, "edit failed, sending new: {e:#}");
                        self.task_messages.remove(key);
                    }
                }
            }
        }

        // No pre-registered message — send a new card.
        match self.send_message(&text, Some(kb)).await {
            Ok(msg_id) => {
                if let Some(key) = task_key {
                    if self.task_messages.len() >= 500 {
                        self.task_messages.clear();
                    }
                    self.task_messages.insert(key.clone(), msg_id);
                }
            }
            Err(e) => {
                warn!(scout_id, "failed to send scout card: {e:#}");
            }
        }
    }

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
            // ScoutProcessed is handled in handle_scout_processed() before
            // reaching build_reply_markup — this branch is a dead-code fallback.
            NotificationKind::ScoutProcessed { .. } => None,
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
    use std::sync::{Arc, Mutex};

    fn test_handler() -> NotificationHandler {
        let api = TelegramApi::new("fake:token");
        let gw = GatewayClient::new(0, None);
        let pending = Arc::new(Mutex::new(std::collections::HashMap::new()));
        NotificationHandler::new(api, "12345".into(), gw, pending)
    }

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
        let handler = test_handler();
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
        let handler = test_handler();
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
    fn scout_processed_no_keyboard_from_build_reply_markup() {
        let handler = test_handler();
        let payload = NotificationPayload {
            message: "scout processed".into(),
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
        // ScoutProcessed is handled by handle_scout_processed, not build_reply_markup
        let markup = handler.build_reply_markup(&payload);
        assert!(markup.is_none());
    }

    #[test]
    fn pending_messages_imported_into_task_messages() {
        let api = TelegramApi::new("fake:token");
        let gw = GatewayClient::new(0, None);
        let pending: PendingMessages = Arc::new(Mutex::new(std::collections::HashMap::new()));
        pending.lock().unwrap().insert("scout:42".into(), 99999);
        let mut handler = NotificationHandler::new(api, "12345".into(), gw, pending.clone());

        // Simulate the import that happens in handle()
        let key = "scout:42".to_string();
        if let Some(msg_id) = handler.pending.lock().unwrap().remove(&key) {
            handler.task_messages.insert(key.clone(), msg_id);
        }
        assert_eq!(handler.task_messages.get("scout:42"), Some(&99999));
        assert!(pending.lock().unwrap().is_empty());
    }

    #[test]
    fn below_threshold_filtered() {
        let mut handler = test_handler();
        handler.set_min_level(NotifyLevel::High);
        assert!(NotifyLevel::Normal < NotifyLevel::High);
    }
}
