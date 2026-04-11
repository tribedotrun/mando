//! Free helper functions for the assistant bot — extracted from mod.rs for file length.

use serde_json::Value;
use tracing::warn;

use anyhow::Result;
use mando_shared::escape_html;

use crate::bot::TelegramBot;
use crate::gateway_paths as paths;
use crate::PendingMessages;

// ── Scout add with processing progress ─────────────────────────────

/// Add a URL to scout, show "processing...", and register the message so
/// the SSE notification handler can edit it with the full summary card.
pub(crate) async fn add_and_track(
    bot: &TelegramBot,
    chat_id: &str,
    message_id: i64,
    url: &str,
    title: Option<&str>,
) -> Result<()> {
    let body = match title {
        Some(t) => serde_json::json!({"url": url, "title": t}),
        None => serde_json::json!({"url": url}),
    };
    let result = match bot.gw.post(paths::SCOUT_ITEMS, &body).await {
        Ok(r) => r,
        Err(e) => {
            let msg = format!("\u{274c} Failed: {}", escape_html(&e.to_string()));
            let _ = bot
                .api
                .edit_message_text(chat_id, message_id, &msg, Some("HTML"), None)
                .await;
            return Ok(());
        }
    };

    let id = result["id"].as_i64().unwrap_or(0);
    let added = result["added"].as_bool().unwrap_or(false);
    let item_type = result["type"].as_str().unwrap_or("unknown");

    if !added {
        let msg = format!(
            "#{id} already exists (<a href=\"{}\">{item_type}</a>)",
            escape_html(url),
        );
        let _ = bot
            .api
            .edit_message_text(chat_id, message_id, &msg, Some("HTML"), None)
            .await;
        return Ok(());
    }

    // Show "processing..." and register for SSE edit-in-place
    let processing_msg = format!("\u{1f4e5} #{id}: {item_type} \u{2014} processing\u{2026}",);
    let _ = bot
        .api
        .edit_message_text(chat_id, message_id, &processing_msg, Some("HTML"), None)
        .await;

    // Register this message so the SSE notification handler can edit it
    // with the full summary card when processing completes.
    register_pending(&bot.pending_scout_msgs, id, message_id);
    Ok(())
}

pub(crate) fn register_pending(pending: &PendingMessages, scout_id: i64, message_id: i64) {
    let key = format!("scout:{scout_id}");
    pending.lock().unwrap().insert(key, message_id);
}

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

    // Single URL: use the tracked add flow with processing progress
    if urls.len() == 1 {
        let sent = bot
            .api
            .send_message(chat_id, "\u{23f3} Adding\u{2026}", Some("HTML"), None, true)
            .await?;
        let mid = sent["message_id"].as_i64().unwrap_or(0);
        add_and_track(bot, chat_id, mid, urls[0], None).await?;
        return Ok(());
    }

    // Multiple URLs: add all, then poll each
    let sent = bot
        .api
        .send_message(
            chat_id,
            &format!("\u{23f3} Adding {} links\u{2026}", urls.len()),
            Some("HTML"),
            None,
            true,
        )
        .await?;
    let message_id = sent["message_id"].as_i64().unwrap_or(0);

    let mut lines = Vec::new();

    for url in &urls {
        let body = serde_json::json!({"url": url});
        match bot.gw.post(paths::SCOUT_ITEMS, &body).await {
            Ok(result) => {
                let id = result["id"].as_i64().unwrap_or(0);
                let added = result["added"].as_bool().unwrap_or(false);
                let item_type = result["type"].as_str().unwrap_or("unknown");
                if added {
                    lines.push(format!(
                        "\u{1f4e5} #{id}: {item_type} \u{2014} processing in background"
                    ));
                } else {
                    lines.push(format!("#{id} already exists"));
                }
            }
            Err(e) => {
                warn!(url = %url, error = %e, "implicit addlink failed");
                lines.push(format!("\u{274c} failed: {}", escape_html(url)));
            }
        }
    }

    let _ = bot
        .api
        .edit_message_text(chat_id, message_id, &lines.join("\n"), Some("HTML"), None)
        .await;
    Ok(())
}
