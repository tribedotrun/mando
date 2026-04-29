//! Telegram bot core — session state container and per-message dispatch.
//!
//! The polling loop and spawn-per-update machinery live in
//! [`crate::bot_runtime`]; type definitions live in [`crate::bot_types`].
//!
//! `TelegramBot` is held behind `Arc` so each incoming update can be
//! dispatched on its own task without blocking the polling loop. All
//! pending-session state moves through `tokio::sync::Mutex`-guarded maps so
//! handlers take `&TelegramBot` rather than `&mut TelegramBot`.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use serde_json::Value;
use tokio::sync::{Mutex, RwLock};
use tracing::{error, info};

use settings::Config;

use crate::api::TelegramApi;
use crate::bot_helpers::{extract_chat_id, extract_photo_todo, extract_user_id, parse_command};
use crate::callbacks;
use crate::commands;
use crate::gateway_paths as paths;
use crate::http::GatewayClient;
use crate::permissions;
use crate::PendingMessages;

pub(crate) use crate::bot_types::SessionKind;
pub use crate::bot_types::{
    ActSession, InputSession, PendingAction, PickerItem, PickerState, PromptMeta, QaSession,
    Session, TodoConfirmState, TodoItem,
};

// ── Macro for repetitive picker store/take ───────────────────────────

/// Defines `pub async fn $store(...)` and `pub async fn $take(...)` for a
/// picker-state map. The stored entry is keyed by `action_id` (callback-data
/// payload), not chat_id, so it does not participate in reply-disambiguation.
macro_rules! picker_methods {
    ($store:ident, $take:ident, $field:ident) => {
        pub async fn $store(&self, action_id: &str, chat_id: &str, items: &[&captain::Task]) {
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
/// without blocking the polling loop. Eight chat_id-keyed pending-session
/// maps and two callback-id-keyed picker maps are wrapped in `tokio::Mutex`
/// for shared interior mutability.
pub struct TelegramBot {
    pub(crate) api: TelegramApi,
    config: Arc<RwLock<Config>>,
    pub(crate) gw: GatewayClient,

    // Chat_id-keyed pending-session maps. Every entry carries a
    // `PromptMeta` so plain-text replies can be routed by reply target
    // (`reply_to_message.message_id`) or by most-recent-wins fallback.
    pub(crate) pending_todo: Mutex<HashMap<String, PromptMeta>>,
    pub(crate) pending_timeline: Mutex<HashMap<String, PromptMeta>>,
    pub(crate) pending_scout_add: Mutex<HashMap<String, PromptMeta>>,
    pub(crate) pending_scout_research: Mutex<HashMap<String, PromptMeta>>,
    pub(crate) ask_sessions: Mutex<HashMap<String, Session>>,
    pub(crate) input_sessions: Mutex<HashMap<String, InputSession>>,
    pub(crate) pending_reopen: Mutex<HashMap<String, PendingAction>>,
    pub(crate) pending_rework: Mutex<HashMap<String, PendingAction>>,
    pub(crate) pending_nudge: Mutex<HashMap<String, PendingAction>>,
    pub(crate) qa_sessions: Mutex<HashMap<String, QaSession>>,
    pub(crate) act_sessions: Mutex<HashMap<String, ActSession>>,

    // Callback-id-keyed pickers; not chat_id-scoped, so out of scope for
    // reply-disambiguation.
    todo_confirm: Mutex<HashMap<String, TodoConfirmState>>,
    action_pickers: Mutex<HashMap<String, PickerState>>,

    /// Shared with `NotificationHandler` — scout "processing..." message IDs
    /// so SSE notifications can edit them in-place with the full card.
    pub(crate) pending_scout_msgs: PendingMessages,
}

impl TelegramBot {
    pub fn new(config: Arc<RwLock<Config>>, token: &str, gw: GatewayClient) -> Self {
        let pending = std::sync::Arc::new(std::sync::Mutex::new(HashMap::new()));
        Self::with_base_url(config, token, None, gw, pending)
            .unwrap_or_else(|e| global_infra::unrecoverable!("default TelegramApi creation", e))
    }

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
            pending_todo: Mutex::new(HashMap::new()),
            pending_timeline: Mutex::new(HashMap::new()),
            pending_scout_add: Mutex::new(HashMap::new()),
            pending_scout_research: Mutex::new(HashMap::new()),
            ask_sessions: Mutex::new(HashMap::new()),
            input_sessions: Mutex::new(HashMap::new()),
            pending_reopen: Mutex::new(HashMap::new()),
            pending_rework: Mutex::new(HashMap::new()),
            pending_nudge: Mutex::new(HashMap::new()),
            qa_sessions: Mutex::new(HashMap::new()),
            act_sessions: Mutex::new(HashMap::new()),
            todo_confirm: Mutex::new(HashMap::new()),
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
        let just_registered = if tg_config.owner.is_empty() {
            self.auto_register_owner(&user_id, &chat_id).await?;
            true
        } else {
            if !permissions::is_owner(&tg_config, &user_id) {
                return Ok(());
            }
            false
        };

        // Photo + /todo caption — extract before text-only dispatch
        let result = if let Some(photo_fid) = extract_photo_todo(message) {
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
        };

        // Spawn SSE notification listener now that we have an owner.
        if just_registered {
            info!("Owner registered — starting SSE notification listener");
            let base_url = self.gw.base_url().to_string();
            let gw_token = self.gw.token().map(String::from);
            let config = self.config.read().await;
            let tg = &config.channels.telegram;
            let api_base_url = crate::resolve_api_base_url();
            let api = match &api_base_url {
                Some(url) => TelegramApi::with_base_url(&tg.token, url)?,
                None => TelegramApi::new(&tg.token),
            };
            let owner_chat_id = chat_id.clone();
            let sse_gw = self.gw.clone();
            let sse_pending = self.pending_scout_msgs.clone();
            // TRACKED: SSE notification loop for the telegram bot process.
            // mando-tg runs as a separate OS process from the gateway, so it
            // has no access to the gateway's TaskTracker. The loop exits when
            // the SSE mpsc receiver drops on bot shutdown.
            tokio::spawn(async move {
                crate::sse::run_notification_loop(
                    base_url,
                    gw_token,
                    api,
                    owner_chat_id,
                    sse_gw,
                    sse_pending,
                )
                .await;
            });
        }

        result
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
            // callback-opened reopen/rework/nudge/ask/input/qa/act) survive,
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
    /// The caller is responsible for restarting the process after the current
    /// command finishes so the SSE notification listener picks up the new owner.
    async fn auto_register_owner(&self, user_id: &str, chat_id: &str) -> Result<()> {
        info!(user_id, chat_id, "Auto-registering bot owner");
        let save_result = self
            .gw
            .post_typed::<_, api_types::BoolOkResponse>(
                paths::CHANNELS_TELEGRAM_OWNER,
                &serde_json::json!({ "owner": user_id }),
            )
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

    // ── Todo confirm ─────────────────────────────────────────────────

    pub async fn store_todo_confirm(
        &self,
        aid: &str,
        cid: &str,
        items: Vec<TodoItem>,
        picker_slugs: Vec<String>,
    ) {
        let mut map = self.todo_confirm.lock().await;
        map.insert(
            aid.to_string(),
            TodoConfirmState {
                chat_id: cid.to_string(),
                items,
                picker_slugs,
            },
        );
    }
    pub async fn take_todo_confirm(&self, aid: &str) -> Option<TodoConfirmState> {
        let mut map = self.todo_confirm.lock().await;
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

    // Session helpers (input/ask/qa/act/pending_*) live in `bot_sessions.rs`.
    // Disambiguation lookup helpers live in `bot_sessions.rs` as well.
}
