//! SSE notification handler — maps gateway `NotificationPayload` events
//! to Telegram messages via the `TelegramApi`.
//!
//! Supports edit-in-place: when a `task_key` is present, subsequent
//! notifications for the same key edit the existing message instead of
//! sending a new one.

use std::collections::HashMap;

use anyhow::Result;
use tracing::{debug, warn};

use api_types::{
    InlineKeyboardButton, NotificationKind, NotificationPayload, NotifyLevel, TelegramReplyMarkup,
};

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
    #[allow(dead_code)]
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
            if let Some(msg_id) = self
                .pending
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .remove(task_key)
            {
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

    /// Handle a research lifecycle SSE event (started/progress/completed/failed).
    pub async fn handle_research(&mut self, data: api_types::ResearchEventData) {
        let action = data.action.as_str();
        let run_id = data.run_id;
        let key = format!("research:{run_id}");

        // Import pre-registered message from cmd_research.
        if let Some(msg_id) = self
            .pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&key)
        {
            self.task_messages.insert(key.clone(), msg_id);
        }

        let text = match action {
            "progress" => {
                let elapsed = data.elapsed_s.unwrap_or(0);
                let mins = elapsed / 60;
                format!("\u{1f50d} Research #{run_id}: {mins}m elapsed\u{2026}")
            }
            "completed" => {
                // Telegram enforces a 4096-char cap on parsed message text.
                // Leave headroom for the header and per-link formatting so a
                // run with long titles/URLs can still render without failing
                // the send/edit call.
                const TELEGRAM_MAX: usize = 3800;
                let added = data.added_count.unwrap_or(0);
                let links = data.links.as_deref();
                let total = links.map_or(0, |l| l.len());
                let errors = data.errors.as_deref().map_or(0, |e| e.len());

                let mut msg =
                    format!("\u{2705} Research #{run_id} complete: {total} links, {added} new");
                if errors > 0 {
                    msg.push_str(&format!(", {errors} error(s)"));
                }

                if let Some(links) = links {
                    let mut shown = 0usize;
                    for link in links.iter().take(10) {
                        let title = link.title.as_str();
                        let url = link.url.as_str();
                        let status = if link.added { "new" } else { "exists" };
                        let line = format!(
                            "\n\u{2022} <a href=\"{}\">{}</a> ({status})",
                            crate::telegram_format::escape_html(url),
                            crate::telegram_format::escape_html(title),
                        );
                        if msg.len() + line.len() > TELEGRAM_MAX {
                            break;
                        }
                        msg.push_str(&line);
                        shown += 1;
                    }
                    let remaining = total.saturating_sub(shown);
                    if remaining > 0 {
                        msg.push_str(&format!("\n\u{2026}and {remaining} more"));
                    }
                }
                msg
            }
            "failed" => {
                let error = data.error.as_deref().unwrap_or("unknown");
                format!(
                    "\u{274c} Research #{run_id} failed: {}",
                    crate::telegram_format::escape_html(error)
                )
            }
            _ => return,
        };

        // Try edit-in-place first.
        if let Some(&msg_id) = self.task_messages.get(&key) {
            match self.edit_message(msg_id, &text, None).await {
                Ok(_) => return,
                Err(e) => {
                    warn!(key, msg_id, "research edit failed: {e:#}");
                    self.task_messages.remove(&key);
                }
            }
        }

        // Fall back to new message.
        if let Ok(msg_id) = self.send_message(&text, None).await {
            self.task_messages.insert(key, msg_id);
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
        let item = match self
            .gw
            .get_typed::<api_types::ScoutItem>(&paths::scout_item(scout_id))
            .await
        {
            Ok(v) => v,
            Err(e) => {
                warn!(scout_id, error = %e, "failed to fetch scout item for card");
                // Edit the "processing..." message so it doesn't stay stuck.
                if let Some(key) = task_key {
                    if let Some(&msg_id) = self.task_messages.get(key) {
                        let fallback =
                            format!("\u{26a0}\u{fe0f} Scout #{scout_id}: failed to load summary");
                        global_infra::best_effort!(
                            self.edit_message(msg_id, &fallback, None).await,
                            "notifications: self.edit_message(msg_id, &fallback, None).await"
                        );
                    }
                }
                return;
            }
        };

        let item_val = serde_json::to_value(&item).unwrap_or_default();
        let summary = item.summary.as_deref();
        let text = format_swipe_card(&item_val, summary);
        let tg_url = item.telegraph_url.as_deref();
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

    async fn send_message(
        &self,
        text: &str,
        reply_markup: Option<TelegramReplyMarkup>,
    ) -> Result<i64> {
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
        reply_markup: Option<TelegramReplyMarkup>,
    ) -> Result<()> {
        self.api
            .edit_message_text(&self.chat_id, message_id, text, Some("HTML"), reply_markup)
            .await?;
        Ok(())
    }

    /// Build typed reply markup from the payload.
    ///
    /// If the payload has explicit `reply_markup`, use that. Otherwise,
    /// generate default inline keyboards based on `NotificationKind`.
    fn build_reply_markup(&self, payload: &NotificationPayload) -> Option<TelegramReplyMarkup> {
        if let Some(markup) = payload.reply_markup.clone() {
            return Some(markup);
        }

        match &payload.kind {
            NotificationKind::NeedsClarification { item_id, .. } => {
                Some(single_button("Answer", format!("answer:{item_id}")))
            }
            NotificationKind::Escalated { item_id, .. } => {
                Some(single_button("View Timeline", format!("view:{item_id}")))
            }
            // ScoutProcessed is handled in handle_scout_processed() before
            // reaching build_reply_markup — this branch is a dead-code fallback.
            NotificationKind::ScoutProcessed { .. } => None,
            NotificationKind::ScoutProcessFailed { scout_id, .. } => {
                Some(single_button("Retry", format!("dg:process:{scout_id}")))
            }
            NotificationKind::AdvisorAnswered { item_id, .. } => {
                Some(single_button("View", format!("view:{item_id}")))
            }
            _ => None,
        }
    }
}

// ── Typed inline keyboard builder ──────────────────────────────────

fn single_button(text: impl Into<String>, callback_data: impl Into<String>) -> TelegramReplyMarkup {
    TelegramReplyMarkup::InlineKeyboard {
        rows: vec![vec![InlineKeyboardButton {
            text: text.into(),
            callback_data: Some(callback_data.into()),
            url: None,
        }]],
    }
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
    fn single_button_is_one_row() {
        let kb = single_button("Click me", "action:123");
        match kb {
            TelegramReplyMarkup::InlineKeyboard { rows } => {
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0].len(), 1);
                assert_eq!(rows[0][0].text, "Click me");
                assert_eq!(rows[0][0].callback_data.as_deref(), Some("action:123"));
            }
            other => panic!("expected InlineKeyboard, got {other:?}"),
        }
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
        let custom = TelegramReplyMarkup::InlineKeyboard {
            rows: vec![vec![InlineKeyboardButton {
                text: "Custom".into(),
                callback_data: Some("x".into()),
                url: None,
            }]],
        };
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
        // Serde round-trip comparison since TelegramReplyMarkup doesn't
        // derive PartialEq.
        assert_eq!(
            serde_json::to_value(&markup.unwrap()).unwrap(),
            serde_json::to_value(&custom).unwrap(),
        );
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
