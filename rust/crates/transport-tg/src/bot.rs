//! Telegram bot core — session state container and per-message dispatch.
//!
//! The polling loop and spawn-per-update machinery live in
//! [`crate::bot_runtime`]; type definitions live in [`crate::bot_types`].
//!
//! `TelegramBot` is held behind `Arc` so each incoming update can be
//! dispatched on its own task without blocking the polling loop. All
//! pending-session and task-scoped append state moves through one typed
//! `tokio::sync::Mutex` registry so handlers take `&TelegramBot` rather than
//! `&mut TelegramBot`.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use serde_json::Value;
use tokio::sync::{Mutex, RwLock};
use tracing::{error, info};

use settings::Config;

use crate::api::TelegramApi;
use crate::bot_helpers::{extract_chat_id, extract_photo_todo, extract_user_id, parse_command};
use crate::bot_sessions::PendingSessionRegistry;
use crate::callbacks;
use crate::commands;
use crate::http::GatewayClient;
use crate::permissions;
use crate::PendingMessages;

pub(crate) use crate::bot_types::SessionKind;
pub use crate::bot_types::{
    ActSession, InputSession, PendingAction, PickerItem, PickerState, PromptMeta, QaSession,
    TodoItem, TodoProjectState,
};

// ── Macro for repetitive picker store/take ───────────────────────────

/// Defines `pub async fn $store(...)` and `pub async fn $take(...)` for a
/// picker-state map. The stored entry is keyed by `action_id` (callback-data
/// payload), not chat_id, so it does not participate in reply-disambiguation.
macro_rules! picker_methods {
    ($store:ident, $take:ident, $field:ident) => {
        pub async fn $store(&self, action_id: &str, chat_id: &str, items: &[&api_types::TaskItem]) {
            let mut map = self.$field.lock().await;
            map.insert(
                action_id.to_string(),
                crate::bot_helpers::to_picker_state(chat_id, items),
            );
            self.save_picker_state_locked(&map);
        }

        pub async fn $take(&self, action_id: &str) -> Option<PickerState> {
            let mut map = self.$field.lock().await;
            let result = map.remove(action_id);
            if result.is_some() {
                self.save_picker_state_locked(&map);
            }
            result
        }
    };
}

/// The Telegram bot — owns session state and the raw API clients.
///
/// Held behind `Arc` so each incoming update can run on its own tokio task
/// without blocking the polling loop. One pending-session registry owns both
/// chat-scoped flows and task-scoped context-append locks; two callback-id-keyed
/// picker maps remain separate.
pub struct TelegramBot {
    pub(crate) api: TelegramApi,
    config: Arc<RwLock<Config>>,
    pub(crate) gw: GatewayClient,

    // Chat_id-keyed pending-session registry. Every entry carries a
    // `PromptMeta` so plain-text replies can be routed by reply target
    // (`reply_to_message.message_id`) or by most-recent-wins fallback.
    pub(crate) pending_sessions: Mutex<PendingSessionRegistry>,

    // Callback-id-keyed pickers; not chat_id-scoped, so out of scope for
    // reply-disambiguation.
    todo_projects: Mutex<HashMap<String, TodoProjectState>>,
    action_pickers: Mutex<HashMap<String, PickerState>>,

    /// Shared with `NotificationHandler` — scout "processing..." message IDs
    /// so SSE notifications can edit them in-place with the full card.
    pub(crate) pending_scout_msgs: PendingMessages,
}

impl TelegramBot {
    pub fn with_base_url(
        config: Arc<RwLock<Config>>,
        token: &str,
        api_base_url: Option<&str>,
        gw: GatewayClient,
        pending_scout_msgs: PendingMessages,
    ) -> anyhow::Result<Self> {
        let api = match api_base_url {
            Some(url) => TelegramApi::with_base_url(token, url)?,
            None => TelegramApi::new(token),
        };
        Ok(Self {
            api,
            config,
            gw,
            pending_sessions: Mutex::new(PendingSessionRegistry::default()),
            todo_projects: Mutex::new(HashMap::new()),
            action_pickers: Mutex::new(HashMap::new()),
            pending_scout_msgs,
        })
    }

    pub(crate) async fn handle_update(self: &Arc<Self>, update: Value) -> Result<()> {
        if let Some(cb) = update.get("callback_query") {
            return callbacks::handle_callback(self, cb).await;
        }
        if let Some(message) = update.get("message") {
            return self.handle_message(message).await;
        }
        Ok(())
    }

    async fn handle_message(self: &Arc<Self>, message: &Value) -> Result<()> {
        let chat_id = extract_chat_id(message);
        let user_id = extract_user_id(message);

        // DM-only: silently ignore group chats
        let chat_type = message
            .get("chat")
            .and_then(|c| c.get("type"))
            .and_then(|t| t.as_str())
            .unwrap_or("private");
        if chat_type == "group" || chat_type == "supergroup" {
            return Ok(());
        }

        let tg_config = self.config.read().await.channels.telegram.clone();

        // Owner-only (auto-register on first message when no owner configured)
        if tg_config.owner.is_empty() {
            self.auto_register_owner(&user_id, &chat_id).await?;
        } else if !permissions::is_owner(&tg_config, &user_id) {
            return Ok(());
        }

        // Photo + /todo caption — extract before text-only dispatch
        if let Some(photo_fid) = extract_photo_todo(message) {
            let caption = message
                .get("caption")
                .and_then(|c| c.as_str())
                .unwrap_or("");
            let (command, args) = parse_command(caption);
            if command == "todo" && !args.is_empty() {
                self.take_pending_todo(&chat_id).await;
                commands::todo::execute_todo_with_photo(self, &chat_id, args, Some(photo_fid)).await
            } else {
                self.dispatch_text(message, &chat_id).await
            }
        } else {
            self.dispatch_text(message, &chat_id).await
        }
    }

    /// Dispatch a text message to the appropriate command handler.
    ///
    /// A `/command` no longer wipes unrelated pending sessions; only
    /// re-issuing `/todo` resets a prior `/todo` pending. The previous
    /// blanket wipe was the concurrency bug: a slow `/scout_research` plus a
    /// quick `/help` would silently kill the research's pending follow-up.
    async fn dispatch_text(self: &Arc<Self>, message: &Value, chat_id: &str) -> Result<()> {
        let text = message.get("text").and_then(|t| t.as_str()).unwrap_or("");

        if text.starts_with('/') {
            let (command, args) = parse_command(text);
            // Re-issuing the same text-command resets its own pending entry
            // so the user can restart a flow. Unrelated pendings (especially
            // callback-opened reopen/rework/nudge/input/qa/act) survive,
            // which is the concurrency-correct behavior.
            self.reset_same_command_pending(chat_id, &command).await;
            return self.dispatch_command(chat_id, &command, args).await;
        }

        self.handle_plain_text(chat_id, text, message).await
    }

    // ── Owner auto-registration ────────────────────────────────────────

    /// Auto-register the first DM sender as the bot owner.
    ///
    /// Called when `config.channels.telegram.owner` is empty and a user
    /// sends any message in a direct chat. Persists the owner to config.json.
    /// The gateway's `TelegramRuntime` starts the sole SSE notification listener
    /// while handling the owner-registration request.
    async fn auto_register_owner(&self, user_id: &str, chat_id: &str) -> Result<()> {
        info!(user_id, chat_id, "Auto-registering bot owner");
        let save_result = self
            .gw
            .post_channels_telegram_owner(&api_types::TelegramOwnerRequest {
                owner: user_id.to_string(),
            })
            .await;
        if let Err(e) = save_result {
            error!("Failed to persist owner to config: {e}");
            let msg = "Registration failed: could not persist owner to config. \
                 Please retry — if this keeps happening, check the daemon logs.";
            global_infra::best_effort!(
                self.api
                    .send_message(chat_id, msg, Some("HTML"), None, true)
                    .await,
                "auto_register_owner: owner-save failure notice"
            );
            return Err(anyhow::anyhow!(
                "auto_register_owner: save_config failed: {e}"
            ));
        }
        self.config.write().await.channels.telegram.owner = user_id.to_string();
        Ok(())
    }

    // ── Public accessors ─────────────────────────────────────────────

    pub fn api(&self) -> &TelegramApi {
        &self.api
    }
    pub fn config(&self) -> &Arc<RwLock<Config>> {
        &self.config
    }
    pub fn gw(&self) -> &GatewayClient {
        &self.gw
    }

    /// Send a loading placeholder and return its `message_id` for later editing.
    pub async fn send_loading(&self, chat_id: &str, text: &str) -> Result<i64> {
        let resp = self.send_html(chat_id, text).await?;
        resp.get("message_id")
            .and_then(|v| v.as_i64())
            .ok_or_else(|| anyhow::anyhow!("response missing message_id"))
    }

    pub async fn send_html(&self, chat_id: &str, text: &str) -> Result<Value> {
        self.api
            .send_message(chat_id, text, Some("HTML"), None, true)
            .await
    }

    pub async fn edit_message(&self, chat_id: &str, mid: i64, text: &str) -> Result<Value> {
        self.api
            .edit_message_text(chat_id, mid, text, Some("HTML"), None)
            .await
    }

    pub async fn edit_message_with_markup(
        &self,
        chat_id: &str,
        mid: i64,
        text: &str,
        reply_markup: Option<api_types::TelegramReplyMarkup>,
    ) -> Result<Value> {
        self.api
            .edit_message_text(chat_id, mid, text, Some("HTML"), reply_markup)
            .await
    }

    /// Send a photo by public URL with an optional HTML caption.
    pub async fn send_photo_url(
        &self,
        chat_id: &str,
        url: &str,
        caption: Option<&str>,
    ) -> Result<Value> {
        self.api
            .send_photo(
                chat_id,
                crate::api::PhotoInput::Url(url.to_string()),
                caption,
                caption.map(|_| "HTML"),
            )
            .await
    }

    /// Send a photo by uploading raw bytes with an optional HTML caption.
    pub async fn send_photo_bytes(
        &self,
        chat_id: &str,
        data: Vec<u8>,
        filename: &str,
        caption: Option<&str>,
    ) -> Result<Value> {
        self.api
            .send_photo(
                chat_id,
                crate::api::PhotoInput::Bytes {
                    data,
                    filename: filename.to_string(),
                },
                caption,
                caption.map(|_| "HTML"),
            )
            .await
    }

    /// Remove the inline keyboard from a message without changing its text.
    pub async fn remove_keyboard(&self, chat_id: &str, mid: i64) -> Result<Value> {
        self.api
            .edit_message_reply_markup(
                chat_id,
                mid,
                Some(api_types::TelegramReplyMarkup::InlineKeyboard { rows: vec![] }),
            )
            .await
    }

    // ── Todo project selection ───────────────────────────────────────

    pub async fn store_todo_project(&self, aid: &str, item: TodoItem, picker_slugs: Vec<String>) {
        let mut map = self.todo_projects.lock().await;
        map.insert(aid.to_string(), TodoProjectState { item, picker_slugs });
    }
    pub async fn take_todo_project(&self, aid: &str) -> Option<TodoProjectState> {
        let mut map = self.todo_projects.lock().await;
        map.remove(aid)
    }

    // Picker state — persisted to ~/.mando/state/picker-state.json (#359).
    picker_methods!(store_action_picker, take_action_picker, action_pickers);

    /// Persist all picker state to disk while the lock is already held.
    pub(crate) fn save_picker_state_locked(&self, map: &HashMap<String, PickerState>) {
        let json = crate::picker_store::collect_json(map);
        crate::picker_store::save(&json);
    }

    /// Load picker state from disk on startup (called once before polling).
    pub async fn load_picker_state(&self) {
        if let Some(maps) = crate::picker_store::load() {
            let mut current = self.action_pickers.lock().await;
            *current = maps.action;
        }
    }

    // Session helpers (input/qa/act/pending_*) live in `bot_sessions.rs`.
    // Disambiguation lookup helpers live in `bot_sessions.rs` as well.
}
