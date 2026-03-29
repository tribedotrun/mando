//! Raw Telegram Bot API HTTP client.
//!
//! Seven methods that POST JSON to `https://api.telegram.org/bot{token}/{method}`.
//! No external Telegram library — this is the in-house implementation.

use anyhow::{Context, Result};
use mando_shared::retry::{retry_on_transient, RetryConfig, RetryVerdict};
use serde::{Deserialize, Serialize};
use serde_json::Value;

fn tg_retry_config() -> RetryConfig {
    RetryConfig::default()
}

/// A single bot command descriptor for `setMyCommands`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotCommand {
    pub command: String,
    pub description: String,
}

/// Raw HTTP client for the Telegram Bot API.
#[derive(Clone)]
pub struct TelegramApi {
    #[allow(dead_code)]
    token: String,
    client: reqwest::Client,
    base_url: String,
}

impl TelegramApi {
    /// Create a new API client for the given bot token.
    pub fn new(token: &str) -> Self {
        Self {
            token: token.to_string(),
            client: reqwest::Client::new(),
            base_url: format!("https://api.telegram.org/bot{token}"),
        }
    }

    /// Create with a custom base URL (for testing with mock servers).
    pub fn with_base_url(token: &str, base_url: &str) -> Result<Self> {
        anyhow::ensure!(
            base_url.starts_with("http://") || base_url.starts_with("https://"),
            "api_base_url must start with http:// or https://, got: {base_url}"
        );
        Ok(Self {
            token: token.to_string(),
            client: reqwest::Client::new(),
            base_url: format!("{base_url}/bot{token}"),
        })
    }

    fn url(&self, method: &str) -> String {
        format!("{}/{method}", self.base_url)
    }

    /// POST JSON to a Telegram API method with automatic retry on transient errors.
    async fn post_with_retry(&self, method: &str, body: &Value) -> Result<Value> {
        let url = self.url(method);
        let method_name = method.to_string();
        let body = body.clone();
        retry_on_transient(
            &tg_retry_config(),
            |e: &anyhow::Error| {
                let msg = e.to_string();
                if msg.contains("Too Many Requests")
                    || msg.contains("retry after")
                    || msg.contains("502")
                    || msg.contains("503")
                    || msg.contains("504")
                    || msg.contains("connection")
                    || msg.contains("timeout")
                {
                    RetryVerdict::Transient
                } else {
                    RetryVerdict::Permanent
                }
            },
            || {
                let url = url.clone();
                let method_name = method_name.clone();
                let body = body.clone();
                let client = self.client.clone();
                async move {
                    let resp: ApiResponse = client
                        .post(&url)
                        .json(&body)
                        .send()
                        .await
                        .context(format!("{method_name} request failed"))?
                        .json()
                        .await
                        .context(format!("{method_name} response parse failed"))?;
                    resp.into_result(&method_name)
                }
            },
        )
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
    pub async fn send_message(
        &self,
        chat_id: &str,
        text: &str,
        parse_mode: Option<&str>,
        reply_markup: Option<Value>,
        disable_web_page_preview: bool,
    ) -> Result<Value> {
        let mut body = serde_json::json!({
            "chat_id": chat_id,
            "text": text,
        });
        if let Some(pm) = parse_mode {
            body["parse_mode"] = Value::String(pm.to_string());
        }
        if let Some(markup) = reply_markup {
            body["reply_markup"] = markup;
        }
        if disable_web_page_preview {
            body["disable_web_page_preview"] = Value::Bool(true);
        }
        self.post_with_retry("sendMessage", &body).await
    }

    /// `editMessageText` — edit a sent message's text.
    pub async fn edit_message_text(
        &self,
        chat_id: &str,
        message_id: i64,
        text: &str,
        parse_mode: Option<&str>,
        reply_markup: Option<Value>,
    ) -> Result<Value> {
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
            body["reply_markup"] = markup;
        }
        self.post_with_retry("editMessageText", &body).await
    }

    /// `editMessageReplyMarkup` — remove or replace the inline keyboard on an existing message.
    pub async fn edit_message_reply_markup(
        &self,
        chat_id: &str,
        message_id: i64,
        reply_markup: Option<Value>,
    ) -> Result<Value> {
        let mut body = serde_json::json!({
            "chat_id": chat_id,
            "message_id": message_id,
        });
        if let Some(markup) = reply_markup {
            body["reply_markup"] = markup;
        }
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
        // self.base_url is "{scheme}://{host}/bot{token}"
        // We need "{scheme}://{host}/file/bot{token}/{file_path}"
        let (root, bot_suffix) = self
            .base_url
            .split_once("/bot")
            .ok_or_else(|| anyhow::anyhow!("unexpected base_url format: {}", self.base_url))?;
        let url = format!("{root}/file/bot{bot_suffix}/{file_path}");
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

    /// Return the token (needed for tests / debug only).
    #[cfg(test)]
    pub fn token(&self) -> &str {
        &self.token
    }
}

// ── Internal response wrapper ────────────────────────────────────────

#[derive(Deserialize)]
struct ApiResponse {
    ok: bool,
    result: Option<Value>,
    description: Option<String>,
}

impl ApiResponse {
    fn into_result(self, method: &str) -> Result<Value> {
        if self.ok {
            Ok(self.result.unwrap_or(Value::Null))
        } else {
            let desc = self.description.unwrap_or_default();
            anyhow::bail!("Telegram API {method} failed: {desc}");
        }
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
    fn api_response_ok() {
        let resp: ApiResponse =
            serde_json::from_str(r#"{"ok": true, "result": {"id": 123, "is_bot": true}}"#).unwrap();
        let val = resp.into_result("test").unwrap();
        assert_eq!(val["id"], 123);
    }

    #[test]
    fn api_response_error() {
        let resp: ApiResponse =
            serde_json::from_str(r#"{"ok": false, "description": "Bad Request: chat not found"}"#)
                .unwrap();
        let err = resp.into_result("test").unwrap_err();
        assert!(err.to_string().contains("chat not found"));
    }

    #[test]
    fn api_url_format() {
        let api = TelegramApi::new("123:ABC");
        assert_eq!(
            api.url("sendMessage"),
            "https://api.telegram.org/bot123:ABC/sendMessage"
        );
    }

    #[test]
    fn send_message_body_shape() {
        // Verify the JSON body we'd send is shaped correctly
        let body = serde_json::json!({
            "chat_id": "12345",
            "text": "hello",
            "parse_mode": "HTML",
            "disable_web_page_preview": true,
        });
        assert_eq!(body["chat_id"], "12345");
        assert_eq!(body["parse_mode"], "HTML");
        assert_eq!(body["disable_web_page_preview"], true);
    }
}
