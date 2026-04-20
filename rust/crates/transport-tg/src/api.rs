//! Raw Telegram Bot API HTTP client.
//!
//! Seven methods that POST JSON to `https://api.telegram.org/bot{token}/{method}`.
//! No external Telegram library — this is the in-house implementation.
//!
//! Error ontology (`TelegramApiError`), retry classification, and response
//! decoding live in the sibling `api_error` module.

use anyhow::{Context, Result};
use global_infra::retry::{retry_on_transient, RetryConfig};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::api_error::{classify_send_error, classify_tg_error, decode_api_response, ApiResponse};

fn tg_retry_config() -> RetryConfig {
    RetryConfig::default()
}

/// Photo source for `sendPhoto`.
#[derive(Clone)]
pub enum PhotoInput {
    /// A public URL that Telegram can download.
    Url(String),
    /// Raw bytes to upload via multipart.
    Bytes { data: Vec<u8>, filename: String },
}

/// A single bot command descriptor for `setMyCommands`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotCommand {
    pub command: String,
    pub description: String,
}

/// Raw HTTP client for the Telegram Bot API.
///
/// The bot token is stored separately from the server root so it never ends
/// up baked into struct fields that could be accidentally logged. URLs are
/// constructed on demand by [`Self::url`].
#[derive(Clone)]
pub struct TelegramApi {
    client: reqwest::Client,
    /// Scheme + host + optional path, e.g. `"https://api.telegram.org"` in
    /// production or `"http://127.0.0.1:PORT"` in tests.
    server_root: String,
    /// Bot token. Never included in Debug output.
    token: String,
}

impl std::fmt::Debug for TelegramApi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TelegramApi")
            .field("server_root", &self.server_root)
            .field("token", &"<redacted>")
            .finish()
    }
}

impl TelegramApi {
    /// Create a new API client for the given bot token.
    pub fn new(token: &str) -> Self {
        Self {
            client: (*global_net::shared_client()).clone(),
            server_root: "https://api.telegram.org".to_string(),
            token: token.to_string(),
        }
    }

    /// Create with a custom base URL (for testing with mock servers).
    pub fn with_base_url(token: &str, base_url: &str) -> Result<Self> {
        anyhow::ensure!(
            base_url.starts_with("http://") || base_url.starts_with("https://"),
            "api_base_url must start with http:// or https://, got: {base_url}"
        );
        Ok(Self {
            client: (*global_net::shared_client()).clone(),
            server_root: base_url.trim_end_matches('/').to_string(),
            token: token.to_string(),
        })
    }

    fn url(&self, method: &str) -> String {
        format!("{}/bot{}/{method}", self.server_root, self.token)
    }

    /// POST JSON to a Telegram API method with automatic retry on transient errors.
    ///
    /// The request body is cloned lazily inside the retry closure so
    /// single-attempt calls (the hot path) never pay for an unused clone.
    async fn post_with_retry(&self, method: &str, body: &Value) -> Result<Value> {
        let url = self.url(method);
        retry_on_transient(&tg_retry_config(), classify_tg_error, || {
            let url = url.clone();
            let body = body.clone();
            let client = self.client.clone();
            async move {
                let resp = client
                    .post(&url)
                    .json(&body)
                    .send()
                    .await
                    .map_err(|e| classify_send_error(e, method))?;
                let api_resp = decode_api_response(resp, method).await?;
                api_resp.into_result(method)
            }
        })
        .await
    }

    /// `getMe` — returns bot info.
    pub async fn get_me(&self) -> Result<Value> {
        self.post_with_retry("getMe", &serde_json::json!({})).await
    }

    /// `getUpdates` — long-poll for new updates.
    pub async fn get_updates(&self, offset: i64, timeout: u64) -> Result<Vec<Value>> {
        let body = serde_json::json!({
            "offset": offset,
            "timeout": timeout,
            "allowed_updates": ["message", "callback_query"],
        });
        let resp: ApiResponse = self
            .client
            .post(self.url("getUpdates"))
            .json(&body)
            .send()
            .await
            .context("getUpdates request failed")?
            .json()
            .await
            .context("getUpdates response parse failed")?;
        let result = resp.into_result("getUpdates")?;
        let updates: Vec<Value> =
            serde_json::from_value(result).context("getUpdates result not an array")?;
        Ok(updates)
    }

    /// `sendMessage` — send a text message.
    ///
    /// `reply_markup` accepts our typed markup; conversion to the Bot API
    /// shape happens through [`crate::telegram_markup::to_bot_api_json`]
    /// inside [`build_send_message_body`].
    pub async fn send_message(
        &self,
        chat_id: &str,
        text: &str,
        parse_mode: Option<&str>,
        reply_markup: Option<api_types::TelegramReplyMarkup>,
        disable_web_page_preview: bool,
    ) -> Result<Value> {
        let body = build_send_message_body(
            chat_id,
            text,
            parse_mode,
            reply_markup,
            disable_web_page_preview,
        );
        self.post_with_retry("sendMessage", &body).await
    }

    /// `editMessageText` — edit a sent message's text.
    pub async fn edit_message_text(
        &self,
        chat_id: &str,
        message_id: i64,
        text: &str,
        parse_mode: Option<&str>,
        reply_markup: Option<api_types::TelegramReplyMarkup>,
    ) -> Result<Value> {
        let body =
            build_edit_message_text_body(chat_id, message_id, text, parse_mode, reply_markup);
        self.post_with_retry("editMessageText", &body).await
    }

    /// `editMessageReplyMarkup` — remove or replace the inline keyboard on an existing message.
    pub async fn edit_message_reply_markup(
        &self,
        chat_id: &str,
        message_id: i64,
        reply_markup: Option<api_types::TelegramReplyMarkup>,
    ) -> Result<Value> {
        let body = build_edit_message_reply_markup_body(chat_id, message_id, reply_markup);
        self.post_with_retry("editMessageReplyMarkup", &body).await
    }

    /// `deleteMessage` — delete a message.
    pub async fn delete_message(&self, chat_id: &str, message_id: i64) -> Result<()> {
        let body = serde_json::json!({
            "chat_id": chat_id,
            "message_id": message_id,
        });
        self.post_with_retry("deleteMessage", &body).await?;
        Ok(())
    }

    /// `answerCallbackQuery` — acknowledge an inline keyboard tap.
    pub async fn answer_callback_query(
        &self,
        callback_query_id: &str,
        text: Option<&str>,
    ) -> Result<()> {
        let mut body = serde_json::json!({
            "callback_query_id": callback_query_id,
        });
        if let Some(t) = text {
            body["text"] = Value::String(t.to_string());
        }
        self.post_with_retry("answerCallbackQuery", &body).await?;
        Ok(())
    }

    /// `getFile` — get file path for downloading.
    pub async fn get_file(&self, file_id: &str) -> Result<String> {
        let body = serde_json::json!({ "file_id": file_id });
        let result = self.post_with_retry("getFile", &body).await?;
        result["file_path"]
            .as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| anyhow::anyhow!("getFile response missing file_path"))
    }

    /// Download file bytes from Telegram's file server.
    ///
    /// `file_path` is the value returned by `getFile` (e.g. "photos/file_42.jpg").
    pub async fn download_file(&self, file_path: &str) -> Result<Vec<u8>> {
        let url = format!("{}/file/bot{}/{file_path}", self.server_root, self.token);
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .context("file download request failed")?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("file download returned {status}: {body}");
        }
        let bytes = resp.bytes().await.context("file download body failed")?;
        Ok(bytes.to_vec())
    }

    /// `sendPhoto` — send a photo via multipart form-data.
    ///
    /// `photo` can be a public URL (Telegram downloads it) or raw bytes.
    /// Returns the Telegram message result.
    pub async fn send_photo(
        &self,
        chat_id: &str,
        photo: PhotoInput,
        caption: Option<&str>,
        parse_mode: Option<&str>,
    ) -> Result<Value> {
        let url = self.url("sendPhoto");
        let chat_id_owned = chat_id.to_string();
        let caption_owned = caption.map(String::from);
        let parse_mode_owned = parse_mode.map(String::from);

        retry_on_transient(&tg_retry_config(), classify_tg_error, || {
            let url = url.clone();
            let client = self.client.clone();
            let chat_id = chat_id_owned.clone();
            let photo = photo.clone();
            let caption = caption_owned.clone();
            let parse_mode = parse_mode_owned.clone();

            async move {
                let mut form = reqwest::multipart::Form::new().text("chat_id", chat_id);

                match photo {
                    PhotoInput::Url(u) => {
                        form = form.text("photo", u);
                    }
                    PhotoInput::Bytes { data, filename } => {
                        let mime = mime_from_filename(&filename);
                        form = form.part(
                            "photo",
                            reqwest::multipart::Part::bytes(data)
                                .file_name(filename)
                                .mime_str(mime)?,
                        );
                    }
                }

                if let Some(c) = caption {
                    form = form.text("caption", c);
                }
                if let Some(pm) = parse_mode {
                    form = form.text("parse_mode", pm);
                }

                let resp = client
                    .post(&url)
                    .multipart(form)
                    .send()
                    .await
                    .map_err(|e| classify_send_error(e, "sendPhoto"))?;
                let api_resp = decode_api_response(resp, "sendPhoto").await?;
                api_resp.into_result("sendPhoto")
            }
        })
        .await
    }

    /// `setMyCommands` — register commands in the Telegram command menu.
    ///
    /// Clears any scope-specific overrides (`all_private_chats`, `all_group_chats`)
    /// so the default-scope commands are what users see everywhere.
    pub async fn set_my_commands(&self, commands: Vec<BotCommand>) -> Result<()> {
        for scope in ["all_private_chats", "all_group_chats"] {
            let body = serde_json::json!({ "scope": { "type": scope } });
            self.post_with_retry("deleteMyCommands", &body).await?;
        }

        let body = serde_json::json!({ "commands": commands });
        self.post_with_retry("setMyCommands", &body).await?;
        Ok(())
    }
}

pub(crate) fn build_send_message_body(
    chat_id: &str,
    text: &str,
    parse_mode: Option<&str>,
    reply_markup: Option<api_types::TelegramReplyMarkup>,
    disable_web_page_preview: bool,
) -> Value {
    let mut body = serde_json::json!({
        "chat_id": chat_id,
        "text": text,
    });
    if let Some(pm) = parse_mode {
        body["parse_mode"] = Value::String(pm.to_string());
    }
    if let Some(markup) = reply_markup {
        body["reply_markup"] = crate::telegram_markup::to_bot_api_json(&markup);
    }
    if disable_web_page_preview {
        body["disable_web_page_preview"] = Value::Bool(true);
    }
    body
}

pub(crate) fn build_edit_message_text_body(
    chat_id: &str,
    message_id: i64,
    text: &str,
    parse_mode: Option<&str>,
    reply_markup: Option<api_types::TelegramReplyMarkup>,
) -> Value {
    let mut body = serde_json::json!({
        "chat_id": chat_id,
        "message_id": message_id,
        "text": text,
        "disable_web_page_preview": true,
    });
    if let Some(pm) = parse_mode {
        body["parse_mode"] = Value::String(pm.to_string());
    }
    if let Some(markup) = reply_markup {
        body["reply_markup"] = crate::telegram_markup::to_bot_api_json(&markup);
    }
    body
}

pub(crate) fn build_edit_message_reply_markup_body(
    chat_id: &str,
    message_id: i64,
    reply_markup: Option<api_types::TelegramReplyMarkup>,
) -> Value {
    let mut body = serde_json::json!({
        "chat_id": chat_id,
        "message_id": message_id,
    });
    if let Some(markup) = reply_markup {
        body["reply_markup"] = crate::telegram_markup::to_bot_api_json(&markup);
    }
    body
}

fn mime_from_filename(name: &str) -> &'static str {
    match name
        .rsplit('.')
        .next()
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        _ => "image/png",
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bot_command_serializes() {
        let cmd = BotCommand {
            command: "start".into(),
            description: "Show help".into(),
        };
        let json = serde_json::to_value(&cmd).unwrap();
        assert_eq!(json["command"], "start");
        assert_eq!(json["description"], "Show help");
    }

    #[test]
    fn api_url_format() {
        let api = TelegramApi::new("123:ABC");
        assert_eq!(
            api.url("sendMessage"),
            "https://api.telegram.org/bot123:ABC/sendMessage"
        );
    }

    /// Locks in that `send_message` sends the Bot API `inline_keyboard` shape,
    /// not our typed `api_types::TelegramReplyMarkup` shape, as the
    /// `reply_markup` field of the outbound POST body. If the translator is
    /// bypassed anywhere on the send path, this regresses.
    #[test]
    fn send_message_posts_bot_api_shape() {
        use api_types::{InlineKeyboardButton, TelegramReplyMarkup};
        let markup = TelegramReplyMarkup::InlineKeyboard {
            rows: vec![vec![InlineKeyboardButton {
                text: "Accept".into(),
                callback_data: Some("accept:42".into()),
                url: None,
            }]],
        };
        let body = build_send_message_body("12345", "hi", Some("HTML"), Some(markup), true);
        let rm = &body["reply_markup"];
        assert_eq!(
            rm["inline_keyboard"][0][0]["text"], "Accept",
            "outbound body must use Bot API snake_case shape, got: {body}"
        );
        assert_eq!(rm["inline_keyboard"][0][0]["callback_data"], "accept:42");
        assert!(
            rm.get("kind").is_none(),
            "outbound body must not carry our typed TelegramReplyMarkup discriminator"
        );
        assert!(
            rm.get("rows").is_none(),
            "outbound body must not carry our typed `rows` field"
        );
    }

    #[test]
    fn edit_message_text_posts_bot_api_shape() {
        use api_types::{InlineKeyboardButton, TelegramReplyMarkup};
        let markup = TelegramReplyMarkup::InlineKeyboard {
            rows: vec![vec![InlineKeyboardButton {
                text: "View".into(),
                callback_data: Some("view:1".into()),
                url: None,
            }]],
        };
        let body =
            build_edit_message_text_body("12345", 999, "updated", Some("HTML"), Some(markup));
        assert_eq!(
            body["reply_markup"]["inline_keyboard"][0][0]["text"],
            "View"
        );
    }

    #[test]
    fn edit_reply_markup_with_none_omits_field() {
        let body = build_edit_message_reply_markup_body("12345", 999, None);
        assert!(body.get("reply_markup").is_none());
    }

    /// Spins up a minimal axum server that captures the outbound POST body
    /// and asserts `send_message` delivers the Bot API `inline_keyboard`
    /// shape over the wire. This is the E2E complement to
    /// `send_message_posts_bot_api_shape` above.
    #[tokio::test]
    async fn send_message_wire_body_is_bot_api_shape() {
        use std::sync::Arc;

        use axum::routing::post;
        use axum::{extract::State, Json, Router};
        use tokio::sync::Mutex;

        type Captured = Arc<Mutex<Option<Value>>>;

        async fn capture(State(cap): State<Captured>, Json(body): Json<Value>) -> Json<Value> {
            let mut slot = cap.lock().await;
            *slot = Some(body);
            Json(serde_json::json!({
                "ok": true,
                "result": { "message_id": 1 }
            }))
        }

        let captured: Captured = Arc::new(Mutex::new(None));
        let app = Router::new()
            .route("/botTEST/sendMessage", post(capture))
            .with_state(captured.clone());
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });

        let base = format!("http://{addr}");
        let api = TelegramApi::with_base_url("TEST", &base).unwrap();
        let markup = api_types::TelegramReplyMarkup::InlineKeyboard {
            rows: vec![vec![api_types::InlineKeyboardButton {
                text: "Accept".into(),
                callback_data: Some("accept:42".into()),
                url: None,
            }]],
        };
        api.send_message("12345", "hi", Some("HTML"), Some(markup), true)
            .await
            .expect("send_message succeeds");

        let body = captured
            .lock()
            .await
            .clone()
            .expect("server must have captured the outbound body");
        let rm = &body["reply_markup"];
        assert_eq!(
            rm["inline_keyboard"][0][0]["text"], "Accept",
            "wire body must carry Bot API shape, got: {body}"
        );
        assert_eq!(rm["inline_keyboard"][0][0]["callback_data"], "accept:42");
        assert!(rm.get("kind").is_none());
        assert!(rm.get("rows").is_none());

        server.abort();
    }
}
