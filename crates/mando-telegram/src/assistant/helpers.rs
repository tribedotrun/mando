//! Free helper functions for the assistant bot — extracted from mod.rs for file length.

use serde_json::Value;
use tracing::warn;

pub(super) use crate::message_helpers::is_group_chat;
use anyhow::Result;
use mando_shared::escape_html;

use crate::bot::TelegramBot;
use crate::gateway_paths as paths;

// ── Message field extractors ────────────────────────────────────────

// ── Implicit addlink ────────────────────────────────────────────────

pub(crate) async fn handle_implicit_addlink(
    bot: &mut TelegramBot,
    chat_id: &str,
    message: &Value,
) -> Result<()> {
    let text = message.get("text").and_then(|t| t.as_str()).unwrap_or("");
    let urls: Vec<&str> = text
        .split_whitespace()
        .filter(|w| w.starts_with("http://") || w.starts_with("https://"))
        .collect();

    if urls.is_empty() {
        return Ok(());
    }

    let label = if urls.len() == 1 {
        "\u{23f3} Adding\u{2026}".to_string()
    } else {
        format!("\u{23f3} Adding {} links\u{2026}", urls.len())
    };
    let sent = bot
        .api
        .send_message(chat_id, &label, Some("HTML"), None, true)
        .await?;
    let message_id = sent["message_id"].as_i64().unwrap_or(0);

    let mut lines = Vec::new();
    let mut added_ids = Vec::new();

    for url in &urls {
        let body = serde_json::json!({"url": url});
        match bot.gw.post(paths::SCOUT_ITEMS, &body).await {
            Ok(result) => {
                let id = result["id"].as_i64().unwrap_or(0);
                let added = result["added"].as_bool().unwrap_or(false);
                let item_type = result["type"].as_str().unwrap_or("unknown");
                if added {
                    lines.push(format!(
                        "\u{1f4e5} #{id}: <a href=\"{}\">{item_type}</a>",
                        escape_html(url),
                    ));
                    added_ids.push(id);
                } else {
                    lines.push(format!(
                        "#{id} already exists (<a href=\"{}\">{item_type}</a>)",
                        escape_html(url),
                    ));
                }
            }
            Err(e) => {
                warn!(url = %url, error = %e, "implicit addlink failed");
                lines.push(format!(
                    "\u{274c} failed: <a href=\"{}\">{}</a>",
                    escape_html(url),
                    escape_html(url),
                ));
            }
        }
    }

    if added_ids.is_empty() {
        if !lines.is_empty() {
            let _ = bot
                .api
                .edit_message_text(chat_id, message_id, &lines.join("\n"), Some("HTML"), None)
                .await;
        }
    } else if added_ids.len() == 1 {
        super::commands::auto_process_single(
            bot,
            chat_id,
            message_id,
            added_ids[0],
            is_group_chat(message),
        )
        .await;
    } else {
        let _ = bot
            .api
            .edit_message_text(chat_id, message_id, &lines.join("\n"), Some("HTML"), None)
            .await;
        super::commands::auto_process_batch(
            bot,
            chat_id,
            message_id,
            &added_ids,
            is_group_chat(message),
        )
        .await;
    }
    Ok(())
}
