//! Callback query handlers for the assistant bot's inline keyboards.
//!
//! Callback data pattern: `dg:{action}:{item_id}`
//! Actions: show, read, next, save, archive, rm

use anyhow::{Context, Result};
use serde_json::Value;
use tracing::{debug, warn};

use super::act;
use super::formatting::{format_swipe_card, swipe_card_kb, telegraph_read_kb};
use crate::bot::TelegramBot;
use crate::gateway_paths as paths;
use crate::permissions;

/// Handle an incoming callback query on the assistant bot.
pub async fn handle_callback(bot: &mut TelegramBot, cb: &Value) -> Result<()> {
    let config = bot.config().read().await.clone();
    let cb_id = cb.get("id").and_then(|v| v.as_str()).unwrap_or_default();

    let data = cb.get("data").and_then(|v| v.as_str()).unwrap_or_default();

    let chat_id = cb
        .get("message")
        .and_then(|m| m.get("chat"))
        .and_then(|c| c.get("id"))
        .and_then(|v| v.as_i64())
        .map(|id| id.to_string())
        .unwrap_or_default();

    let message_id = cb
        .get("message")
        .and_then(|m| m.get("message_id"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    let is_group = cb
        .get("message")
        .and_then(|m| m.get("chat"))
        .and_then(|c| c.get("type"))
        .and_then(|t| t.as_str())
        .map(|t| t == "group" || t == "supergroup")
        .unwrap_or(false);

    let user_id = {
        let from = cb.get("from");
        let numeric = from
            .and_then(|f| f.get("id"))
            .and_then(|v| v.as_i64())
            .map(|id| id.to_string())
            .unwrap_or_default();
        let username = from
            .and_then(|f| f.get("username"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if username.is_empty() {
            numeric
        } else {
            format!("{numeric}|{username}")
        }
    };

    let tg_config = &config.channels.telegram;
    if !permissions::is_allowed(tg_config, &user_id) {
        bot.api.answer_callback_query(cb_id, None).await?;
        return Ok(());
    }

    let parts: Vec<&str> = data.split(':').collect();
    if parts.len() < 3 || parts[0] != "dg" {
        bot.api.answer_callback_query(cb_id, None).await?;
        return Ok(());
    }

    let action = parts[1];

    // Abandon any pending act session when user interacts with other buttons
    if action != "actskip" && action != "actpick" {
        bot.take_act_session(&chat_id);
    }

    // No-op callback (page indicator button)
    if action == "noop" {
        bot.api.answer_callback_query(cb_id, None).await?;
        return Ok(());
    }

    // Pagination callbacks: dg:page (summary list), dg:cpage (compact list)
    if action == "page" || action == "cpage" {
        bot.api.answer_callback_query(cb_id, None).await?;
        let page: usize = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
        let status_filter = parts.get(3).copied().unwrap_or("");
        if action == "cpage" {
            return super::commands::edit_simplelist_page(
                bot,
                &chat_id,
                message_id,
                status_filter,
                page,
            )
            .await;
        }
        return super::scout_commands::edit_list_page(
            bot,
            &chat_id,
            message_id,
            status_filter,
            page,
        )
        .await;
    }

    // Act project picker: dg:actpick:{item_id}:{project_name}
    if action == "actpick" && parts.len() >= 4 {
        let id = parts[2].parse::<i64>().unwrap_or(0);
        let project = parts[3];
        return act::cb_act_with_project(bot, cb_id, &chat_id, id, project).await;
    }

    // Act skip prompt: dg:actskip:{item_id}
    if action == "actskip" {
        bot.api.answer_callback_query(cb_id, None).await?;
        let cb_item_id = parts
            .get(2)
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0);
        let session = bot.take_act_session(&chat_id);
        if let Some(s) = session {
            if s.item_id != cb_item_id {
                // Stale button from a previous act flow — restore current session
                bot.open_act_session(&chat_id, s.item_id, &s.project);
                bot.api
                    .send_message(
                        &chat_id,
                        "That button is outdated. Use the latest prompt above.",
                        None,
                        None,
                        true,
                    )
                    .await?;
                return Ok(());
            }
            return act::execute_act(bot, &chat_id, s.item_id, &s.project, None).await;
        }
        // Session expired — tell the user instead of silently restarting
        debug!(%chat_id, "act session expired on skip");
        bot.api
            .send_message(
                &chat_id,
                "Session expired. Tap \u{2699}\u{fe0f} Act again to restart.",
                None,
                None,
                true,
            )
            .await?;
        return Ok(());
    }

    let id = match parts[2].parse::<i64>() {
        Ok(id) => id,
        Err(_) => {
            bot.api.answer_callback_query(cb_id, None).await?;
            return Ok(());
        }
    };

    debug!(action = %action, item_id = id, "callback received");

    match action {
        "show" => cb_show(bot, cb_id, &chat_id, message_id, id, is_group).await,
        "read" => cb_read(bot, cb_id, &chat_id, id, is_group).await,
        "next" => cb_next(bot, cb_id, &chat_id, message_id, id, is_group).await,
        "save" => {
            bot.api
                .answer_callback_query(cb_id, Some("Saving\u{2026}"))
                .await?;
            let body = serde_json::json!({"status": "saved"});
            bot.gw().patch(&paths::scout_item(id), &body).await?;
            swipe_next(bot, &chat_id, message_id, id, is_group).await
        }
        "archive" => {
            bot.api
                .answer_callback_query(cb_id, Some("Archiving\u{2026}"))
                .await?;
            let body = serde_json::json!({"status": "archived"});
            bot.gw().patch(&paths::scout_item(id), &body).await?;
            swipe_next(bot, &chat_id, message_id, id, is_group).await
        }
        "rm" => {
            bot.api
                .answer_callback_query(cb_id, Some("Removing\u{2026}"))
                .await?;
            bot.gw().delete(&paths::scout_item(id)).await?;
            swipe_next(bot, &chat_id, message_id, id, is_group).await
        }
        "ask" => {
            bot.api
                .answer_callback_query(cb_id, Some("Q&A started"))
                .await?;
            let title = {
                let item = bot.gw().get(&paths::scout_item(id)).await?;
                item["title"].as_str().unwrap_or("item").to_string()
            };
            bot.open_qa_session(&chat_id, id);
            bot.api
                .send_message(
                    &chat_id,
                    &format!(
                        "\u{1f4ac} Q&A session for <b>#{id}</b>: {}\n\nType your question.",
                        mando_shared::telegram_format::escape_html(&title),
                    ),
                    Some("HTML"),
                    Some(super::formatting::qa_session_kb(id)),
                    true,
                )
                .await?;
            Ok(())
        }
        "act" => act::cb_act(bot, cb_id, &chat_id, id, &config).await,
        "endqa" => {
            bot.close_qa_session(&chat_id);
            bot.api
                .answer_callback_query(cb_id, Some("Session ended"))
                .await?;
            let item = bot.gw().get(&paths::scout_item(id)).await?;
            let summary = item["summary"].as_str();
            let text = format_swipe_card(&item, summary);
            let tg_url = item["telegraphUrl"].as_str();
            let item_type = item["item_type"].as_str();
            let orig_url = item["url"].as_str();
            let kb = swipe_card_kb(id, is_group, tg_url, item_type, orig_url);
            bot.api
                .send_message(&chat_id, &text, Some("HTML"), Some(kb), true)
                .await?;
            Ok(())
        }
        _ => {
            bot.api.answer_callback_query(cb_id, None).await?;
            Ok(())
        }
    }
}

/// Show item card (edit message in place).
async fn cb_show(
    bot: &TelegramBot,
    cb_id: &str,
    chat_id: &str,
    message_id: i64,
    id: i64,
    is_group: bool,
) -> Result<()> {
    bot.api.answer_callback_query(cb_id, None).await?;
    let item = bot.gw().get(&paths::scout_item(id)).await?;
    let summary = item["summary"].as_str();
    let text = format_swipe_card(&item, summary);
    let tg_url = item["telegraphUrl"].as_str();
    let item_type = item["item_type"].as_str();
    let orig_url = item["url"].as_str();
    let kb = swipe_card_kb(id, is_group, tg_url, item_type, orig_url);
    if let Err(e) = bot
        .api
        .edit_message_text(chat_id, message_id, &text, Some("HTML"), Some(kb.clone()))
        .await
    {
        warn!(error = %e, "edit_message_text failed, falling back to send");
        bot.api
            .send_message(chat_id, &text, Some("HTML"), Some(kb), true)
            .await?;
    }
    Ok(())
}

/// Read article — YouTube: open Telegraph link (publish on demand if needed).
/// Non-YouTube: link to original URL.
async fn cb_read(
    bot: &TelegramBot,
    cb_id: &str,
    chat_id: &str,
    id: i64,
    is_group: bool,
) -> Result<()> {
    bot.api
        .answer_callback_query(cb_id, Some("Loading\u{2026}"))
        .await?;

    let item = bot.gw().get(&paths::scout_item(id)).await?;
    let title = item["title"].as_str().unwrap_or("Untitled");
    let item_type = item["item_type"].as_str().unwrap_or("other");

    // Non-YouTube: link directly to the original URL.
    if item_type != "youtube" {
        let url = item["url"].as_str().unwrap_or("");
        let msg = format!(
            "\u{1f4d6} <a href=\"{}\">{}</a>",
            mando_shared::telegram_format::escape_html(url),
            mando_shared::telegram_format::escape_html(title),
        );
        bot.api
            .send_message(chat_id, &msg, Some("HTML"), None, true)
            .await?;
        return Ok(());
    }

    // YouTube: use Telegraph article.
    let article = bot.gw().get(&paths::scout_article(id)).await?;

    let telegraph_url = match article["telegraphUrl"].as_str() {
        Some(url) => url.to_string(),
        None => {
            // No cached URL — publish now via gateway
            let result = bot
                .gw()
                .post(&paths::scout_telegraph(id), &serde_json::json!({}))
                .await?;
            result["url"]
                .as_str()
                .context("telegraph publish returned no URL")?
                .to_string()
        }
    };

    let kb = telegraph_read_kb(id, &telegraph_url, is_group);
    let msg = format!(
        "\u{1f4d6} <a href=\"{}\">{}</a>",
        mando_shared::telegram_format::escape_html(&telegraph_url),
        mando_shared::telegram_format::escape_html(title),
    );
    bot.api
        .send_message(chat_id, &msg, Some("HTML"), Some(kb), true)
        .await?;
    Ok(())
}

/// Next button — advance to next processed item.
async fn cb_next(
    bot: &TelegramBot,
    cb_id: &str,
    chat_id: &str,
    message_id: i64,
    current_id: i64,
    is_group: bool,
) -> Result<()> {
    bot.api.answer_callback_query(cb_id, None).await?;
    swipe_next(bot, chat_id, message_id, current_id, is_group).await
}

/// Advance to the next processed item after the given ID.
///
/// Edits the current message in place when possible.
async fn swipe_next(
    bot: &TelegramBot,
    chat_id: &str,
    message_id: i64,
    after_id: i64,
    is_group: bool,
) -> Result<()> {
    let result = bot.gw().get(&paths::processed_scout_items(10000)).await?;

    // Items are DESC (newest first); "next" = next lower ID
    let next_item = result["items"].as_array().and_then(|items| {
        items
            .iter()
            .find(|item| item["id"].as_i64().unwrap_or(0) < after_id)
    });

    match next_item {
        Some(item) => {
            let id = item["id"].as_i64().unwrap_or(0);
            let full_item = bot.gw().get(&paths::scout_item(id)).await?;
            let summary = full_item["summary"].as_str();
            let text = format_swipe_card(&full_item, summary);
            let tg_url = full_item["telegraphUrl"].as_str();
            let item_type = full_item["item_type"].as_str();
            let orig_url = full_item["url"].as_str();
            let kb = swipe_card_kb(id, is_group, tg_url, item_type, orig_url);

            if let Err(e) = bot
                .api
                .edit_message_text(chat_id, message_id, &text, Some("HTML"), Some(kb.clone()))
                .await
            {
                warn!(error = %e, "edit_message_text failed, falling back to send");
                bot.api
                    .send_message(chat_id, &text, Some("HTML"), Some(kb), true)
                    .await?;
            }
        }
        None => {
            let text = "\u{1f4ed} Inbox zero \u{2014} no more processed items.";
            if let Err(e) = bot
                .api
                .edit_message_text(chat_id, message_id, text, None, None)
                .await
            {
                warn!(error = %e, "edit_message_text failed, falling back to send");
                bot.api
                    .send_message(chat_id, text, None, None, true)
                    .await?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn callback_data_format() {
        let data = "dg:save:42";
        let parts: Vec<&str> = data.split(':').collect();
        assert_eq!(parts[0], "dg");
        assert_eq!(parts[1], "save");
        assert_eq!(parts[2].parse::<i64>().unwrap(), 42);
    }
}
