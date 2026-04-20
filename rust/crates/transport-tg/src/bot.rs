//! Polling loop and command dispatch.
//!
//! `TelegramBot` wraps the raw API, holds session state, and dispatches
//! incoming updates to the correct command/callback handler.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

use anyhow::Result;
use serde_json::Value;
use tracing::{error, info, warn};

use settings::config::settings::Config;

use crate::http::GatewayClient;

use crate::api::TelegramApi;
use crate::bot_helpers::{
    extract_chat_id, extract_photo_todo, extract_user_id, parse_command, to_picker_state,
};
use crate::callbacks;
use crate::commands;
use crate::permissions;
use crate::PendingMessages;

// ── Session state types ──────────────────────────────────────────────

/// Lightweight session tracker for /ask.
#[derive(Debug)]
pub struct Session {
    pub task_id: i64,
    pub rounds: u32,
}

impl Session {
    pub fn new(task_id: i64) -> Self {
        Self { task_id, rounds: 0 }
    }
}

/// Active Q&A session for a chat (scout items).
#[derive(Debug)]
pub struct QaSession {
    pub item_id: i64,
    pub rounds: u32,
    /// CC session ID from first Q&A response — used to resume on follow-ups.
    pub cc_session_id: Option<String>,
}

/// Pending Act session — waiting for optional user prompt (scout items).
#[derive(Debug)]
pub struct ActSession {
    pub item_id: i64,
    pub project: String,
}

/// Picker state stored while an inline keyboard is active.
#[derive(Debug)]
pub struct PickerState {
    pub chat_id: String,
    pub items: Vec<PickerItem>,
    /// Indices of selected items (for multi-select pickers).
    pub selected: std::collections::HashSet<usize>,
}

/// One item in a picker.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct PickerItem {
    pub id: String,
    pub title: String,
    /// Item status string (e.g. "needs-clarification").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Whether this task has a PR attached.
    pub has_pr: bool,
}

/// One parsed todo item with optional project assignment.
#[derive(Debug, Clone)]
pub struct TodoItem {
    pub title: String,
    /// Project slug resolved via prefix match or single-project auto-select.
    pub project: Option<String>,
    /// Telegram photo file_id (highest-res) — only set on first item.
    pub photo_file_id: Option<String>,
}

/// Pending /todo confirmation state.
#[derive(Debug)]
pub struct TodoConfirmState {
    pub chat_id: String,
    pub items: Vec<TodoItem>,
    /// Ordered project slugs for the picker (indices used in callback_data).
    pub picker_slugs: Vec<String>,
}

// ── Macro for repetitive picker store/take ───────────────────────────

macro_rules! picker_methods {
    ($store:ident, $take:ident, $field:ident) => {
        pub fn $store(&mut self, action_id: &str, chat_id: &str, items: &[&captain::Task]) {
            self.$field
                .insert(action_id.to_string(), to_picker_state(chat_id, items));
            self.save_picker_state();
        }

        pub fn $take(&mut self, action_id: &str) -> Option<PickerState> {
            let result = self.$field.remove(action_id);
            if result.is_some() {
                self.save_picker_state();
            }
            result
        }
    };
}

/// The Telegram bot — polling loop, command dispatch, session state.
pub struct TelegramBot {
    pub(crate) api: TelegramApi,
    config: Arc<RwLock<Config>>,
    pub(crate) gw: GatewayClient,
    pub(crate) pending_todo: HashSet<String>,
    pub(crate) ask_sessions: HashMap<String, Session>,
    pub(crate) input_sessions: HashMap<String, String>,
    pub(crate) pending_reopen: HashMap<String, (String, String)>,
    pub(crate) pending_rework: HashMap<String, (String, String)>,
    pub(crate) pending_nudge: HashMap<String, (String, String)>,
    pub(crate) qa_sessions: HashMap<String, QaSession>,
    pub(crate) act_sessions: HashMap<String, ActSession>,
    todo_confirm: HashMap<String, TodoConfirmState>,
    action_pickers: HashMap<String, PickerState>,
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
            pending_todo: HashSet::new(),
            ask_sessions: HashMap::new(),
            input_sessions: HashMap::new(),
            pending_reopen: HashMap::new(),
            pending_rework: HashMap::new(),
            pending_nudge: HashMap::new(),
            qa_sessions: HashMap::new(),
            act_sessions: HashMap::new(),
            todo_confirm: HashMap::new(),
            action_pickers: HashMap::new(),
            pending_scout_msgs,
        })
    }

    /// Main polling loop.
    pub async fn start(&mut self) -> Result<()> {
        // Wait for the gateway to be reachable before processing commands.
        info!("Waiting for gateway at {}", self.gw.base_url());
        self.gw.wait_for_gateway(Duration::from_secs(30)).await?;
        info!("Gateway reachable");

        let me = self.api.get_me().await?;
        let username = me
            .get("username")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        info!("Telegram bot @{username} connected");
        self.load_picker_state();
        self.register_commands().await;

        let mut offset: i64 = 0;
        loop {
            let updates = match self.api.get_updates(offset, 30).await {
                Ok(u) => u,
                Err(e) => {
                    warn!("getUpdates failed: {e}");
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    continue;
                }
            };
            for update in updates {
                if let Some(uid) = update.get("update_id").and_then(|v| v.as_i64()) {
                    offset = uid + 1;
                }
                if let Err(e) = self.handle_update(update).await {
                    error!("Error handling update: {e}");
                }
            }
        }
    }

    async fn handle_update(&mut self, update: Value) -> Result<()> {
        if let Some(cb) = update.get("callback_query") {
            return callbacks::handle_callback(self, cb).await;
        }
        if let Some(message) = update.get("message") {
            return self.handle_message(message).await;
        }
        Ok(())
    }

    async fn handle_message(&mut self, message: &Value) -> Result<()> {
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
                self.pending_todo.remove(&chat_id);
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
    async fn dispatch_text(&mut self, message: &serde_json::Value, chat_id: &str) -> Result<()> {
        let text = message.get("text").and_then(|t| t.as_str()).unwrap_or("");

        if text.starts_with('/') {
            let (command, args) = parse_command(text);
            if command != "todo" {
                self.pending_todo.remove(chat_id);
            }
            self.pending_reopen.remove(chat_id);
            self.pending_rework.remove(chat_id);
            self.pending_nudge.remove(chat_id);
            self.act_sessions.remove(chat_id);
            return self.dispatch_command(chat_id, &command, args).await;
        }

        self.handle_plain_text(chat_id, text, message).await
    }

    // dispatch_command, handle_plain_text, register_commands are in bot_dispatch.rs

    // ── Owner auto-registration ────────────────────────────────────────

    /// Auto-register the first DM sender as the bot owner.
    ///
    /// Called when `config.channels.telegram.owner` is empty and a user
    /// sends any message in a direct chat. Persists the owner to config.json.
    /// The caller is responsible for restarting the process after the current
    /// command finishes so the SSE notification listener picks up the new owner.
    async fn auto_register_owner(&mut self, user_id: &str, chat_id: &str) -> Result<()> {
        info!(user_id, chat_id, "Auto-registering bot owner");
        let save_result = self
            .gw
            .post_typed::<_, api_types::BoolOkResponse>(
                "/api/channels/telegram/owner",
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

    // ── Pending todo ─────────────────────────────────────────────────
    pub fn set_pending_todo(&mut self, chat_id: &str) {
        self.pending_todo.insert(chat_id.to_string());
    }
    pub fn clear_pending_todo(&mut self, chat_id: &str) {
        self.pending_todo.remove(chat_id);
    }

    // ── Todo confirm ─────────────────────────────────────────────────

    pub fn store_todo_confirm(
        &mut self,
        aid: &str,
        cid: &str,
        items: Vec<TodoItem>,
        picker_slugs: Vec<String>,
    ) {
        self.todo_confirm.insert(
            aid.to_string(),
            TodoConfirmState {
                chat_id: cid.to_string(),
                items,
                picker_slugs,
            },
        );
    }
    pub fn take_todo_confirm(&mut self, aid: &str) -> Option<TodoConfirmState> {
        self.todo_confirm.remove(aid)
    }

    // Picker state — persisted to ~/.mando/state/picker-state.json (#359).
    picker_methods!(store_action_picker, take_action_picker, action_pickers);

    /// Persist all picker state to disk.
    pub fn save_picker_state(&self) {
        let json = crate::picker_store::collect_json(&self.action_pickers);
        crate::picker_store::save(&json);
    }

    /// Load picker state from disk on startup.
    pub fn load_picker_state(&mut self) {
        if let Some(maps) = crate::picker_store::load() {
            self.action_pickers = maps.action;
        }
    }

    // Input/ops/ask session methods are in bot_sessions.rs
}
