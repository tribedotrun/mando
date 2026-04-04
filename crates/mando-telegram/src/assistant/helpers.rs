//! Free helper functions for the assistant bot — extracted from mod.rs for file length.

use std::time::Duration;

use serde_json::Value;
use tracing::warn;

use anyhow::Result;
use mando_shared::escape_html;

use crate::api::TelegramApi;
use crate::bot::TelegramBot;
use crate::gateway_paths as paths;
use crate::http::GatewayClient;

// ── Scout add with processing progress ─────────────────────────────

/// Add a URL to scout and poll until processing completes, updating the
/// TG message through each stage: Adding → processing → done/failed.
pub(crate) async fn add_and_track(
    api: &TelegramApi,
    gw: &GatewayClient,
    chat_id: &str,
    message_id: i64,
    url: &str,
    title: Option<&str>,
) -> Result<String> {
    let body = match title {
        Some(t) => serde_json::json!({"url": url, "title": t}),
        None => serde_json::json!({"url": url}),
    };
    let result = match gw.post(paths::SCOUT_ITEMS, &body).await {
        Ok(r) => r,
        Err(e) => {
            let msg = format!("\u{274c} Failed: {}", escape_html(&e.to_string()));
            let _ = api
                .edit_message_text(chat_id, message_id, &msg, Some("HTML"), None)
                .await;
            return Ok(msg);
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
        let _ = api
            .edit_message_text(chat_id, message_id, &msg, Some("HTML"), None)
            .await;
        return Ok(msg);
    }

    // Show "processing..." stage
    let processing_msg = format!("\u{1f4e5} #{id}: {item_type} \u{2014} processing\u{2026}",);
    let _ = api
        .edit_message_text(chat_id, message_id, &processing_msg, Some("HTML"), None)
        .await;

    // Poll until processed (up to 60s)
    let endpoint = paths::scout_item(id);
    for _ in 0..30 {
        tokio::time::sleep(Duration::from_secs(2)).await;
        if let Ok(item) = gw.get(&endpoint).await {
            let status = item["status"].as_str().unwrap_or("pending");
            if status == "processed" {
                let title = item["title"].as_str().unwrap_or(item_type);
                let r = item["relevance"].as_i64().unwrap_or(0);
                let q = item["quality"].as_i64().unwrap_or(0);
                let msg = format!("\u{2705} #{id}: <b>{}</b>\nR:{r} Q:{q}", escape_html(title),);
                let _ = api
                    .edit_message_text(chat_id, message_id, &msg, Some("HTML"), None)
                    .await;
                return Ok(msg);
            }
            if status == "error" {
                let msg = format!("\u{274c} #{id}: processing failed");
                let _ = api
                    .edit_message_text(chat_id, message_id, &msg, Some("HTML"), None)
                    .await;
                return Ok(msg);
            }
        }
    }
    // Timed out — leave the "processing..." message; SSE will notify later
    Ok(processing_msg)
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
        add_and_track(&bot.api, &bot.gw, chat_id, mid, urls[0], None).await?;
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
