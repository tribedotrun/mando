//! Scout-related command handlers for the assistant bot.

use anyhow::Result;

use mando_shared::telegram_format::escape_html;

use super::commands::send_html;
use super::formatting::{format_swipe_card, swipe_card_kb};
use super::helpers::register_pending;
use crate::bot::TelegramBot;
use crate::gateway_paths as paths;

// ── /scout (swipe flow only) ────────────────────────────────────────

pub async fn cmd_scout(bot: &mut TelegramBot, chat_id: &str) -> Result<()> {
    swipe_start(bot, chat_id).await
}

// ── Swipe flow ──────────────────────────────────────────────────────

async fn swipe_start(bot: &TelegramBot, chat_id: &str) -> Result<()> {
    let result = match bot.gw().get(&paths::processed_scout_items(10000)).await {
        Ok(r) => r,
        Err(e) => {
            send_html(
                bot,
                chat_id,
                &format!(
                    "\u{274c} Failed to load scout: {}",
                    escape_html(&e.to_string())
                ),
            )
            .await?;
            return Ok(());
        }
    };
    let first = result["items"].as_array().and_then(|items| items.first());

    match first {
        Some(item) => {
            let id = item["id"].as_i64().unwrap_or(0);
            show_card(bot, chat_id, id).await
        }
        None => {
            send_html(
                bot,
                chat_id,
                "\u{1f4ed} Inbox zero \u{2014} no processed items to review.",
            )
            .await?;
            Ok(())
        }
    }
}

pub async fn show_card(bot: &TelegramBot, chat_id: &str, id: i64) -> Result<()> {
    let item = bot.gw().get(&paths::scout_item(id)).await?;
    let summary = item["summary"].as_str();
    let text = format_swipe_card(&item, summary);
    let tg_url = item["telegraphUrl"].as_str();
    let kb = swipe_card_kb(id, tg_url);

    bot.api
        .send_message(chat_id, &text, Some("HTML"), Some(kb), true)
        .await?;
    Ok(())
}

// ── /scout_research ────────────────────────────────────────────────

pub async fn cmd_research(bot: &mut TelegramBot, chat_id: &str, args: &str) -> Result<()> {
    let topic = args.trim();
    if topic.is_empty() {
        send_html(
            bot,
            chat_id,
            "Usage: /scout_research &lt;topic&gt;\nExample: /scout_research Rust async patterns",
        )
        .await?;
        return Ok(());
    }

    let sent = send_html(
        bot,
        chat_id,
        &format!(
            "\u{1f50d} Researching <b>{}</b>\u{2026}",
            escape_html(topic)
        ),
    )
    .await?;
    let message_id = sent["message_id"].as_i64().unwrap_or(0);

    let body = serde_json::json!({"topic": topic, "process": true});
    match bot.gw().post(paths::SCOUT_RESEARCH, &body).await {
        Ok(result) => {
            let added = result["added"].as_u64().unwrap_or(0);
            let links = result["links"].as_array();

            let mut text = format!(
                "\u{1f50d} Research for <b>{}</b>: found {} links, {added} new\n",
                escape_html(topic),
                links.map_or(0, |l| l.len()),
            );

            if let Some(links) = links {
                for link in links.iter().take(10) {
                    let title = link["title"].as_str().unwrap_or("Untitled");
                    let url = link["url"].as_str().unwrap_or("");
                    let was_added = link["added"].as_bool() == Some(true);
                    let status = if was_added {
                        "processing\u{2026}"
                    } else {
                        "exists"
                    };
                    text.push_str(&format!(
                        "\n\u{2022} <a href=\"{}\">{}</a> ({status})",
                        escape_html(url),
                        escape_html(title),
                    ));
                }
                if links.len() > 10 {
                    text.push_str(&format!("\n\u{2026}and {} more", links.len() - 10));
                }
            }

            if message_id > 0 {
                if let Err(e) = bot
                    .api
                    .edit_message_text(chat_id, message_id, &text, Some("HTML"), None)
                    .await
                {
                    tracing::warn!(error = %e, "edit failed, sending new message");
                    let _ = bot
                        .api
                        .send_message(chat_id, &text, Some("HTML"), None, true)
                        .await;
                }
            }

            // Send per-item "processing..." messages and register each for
            // SSE edit-in-place, matching the add_and_track pattern.
            if let Some(links) = result["links"].as_array() {
                for link in links {
                    let was_added = link["added"].as_bool() == Some(true);
                    let id = link["id"].as_i64().unwrap_or(0);
                    if was_added && id > 0 {
                        let item_type = link["type"].as_str().unwrap_or("link");
                        let msg =
                            format!("\u{1f4e5} #{id}: {item_type} \u{2014} processing\u{2026}");
                        if let Ok(sent) = bot
                            .api
                            .send_message(chat_id, &msg, Some("HTML"), None, true)
                            .await
                        {
                            let mid = sent["message_id"].as_i64().unwrap_or(0);
                            if mid > 0 {
                                register_pending(&bot.pending_scout_msgs, id, mid);
                            }
                        }
                    }
                }
            }
        }
        Err(e) => {
            let text = format!("\u{274c} Research failed: {}", escape_html(&e.to_string()));
            if message_id > 0 {
                if let Err(edit_err) = bot
                    .api
                    .edit_message_text(chat_id, message_id, &text, Some("HTML"), None)
                    .await
                {
                    tracing::warn!(error = %edit_err, "edit failed, sending new message");
                    let _ = bot
                        .api
                        .send_message(chat_id, &text, Some("HTML"), None, true)
                        .await;
                }
            }
        }
    }
    Ok(())
}
