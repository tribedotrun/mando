//! Callback query handlers for the assistant bot's inline keyboards.
//!
//! Callback data pattern: `dg:{action}:{item_id}`
//! Actions: show, read, next, save, archive, rm, process

use anyhow::Result;
use serde_json::Value;
use tracing::{debug, warn};

use super::act;
use super::formatting::{format_swipe_card, swipe_card_kb, telegraph_read_kb};
use crate::bot::TelegramBot;
use crate::permissions;

/// Handle an incoming callback query on the assistant bot.
pub async fn handle_callback(bot: &TelegramBot, cb: &Value) -> Result<()> {
    let config = bot.config().read().await.clone();
    let cb_id = cb.get("id").and_then(|v| v.as_str()).unwrap_or_default();

    let data = cb.get("data").and_then(|v| v.as_str()).unwrap_or_default();

    // DM-only: silently ignore callbacks from group chats.
    let chat_type = cb
        .get("message")
        .and_then(|m| m.get("chat"))
        .and_then(|c| c.get("type"))
        .and_then(|t| t.as_str())
        .unwrap_or("private");
    if chat_type == "group" || chat_type == "supergroup" {
        bot.api.answer_callback_query(cb_id, None).await?;
        return Ok(());
    }

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
    if !permissions::is_owner(tg_config, &user_id) {
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
        bot.take_act_session(&chat_id).await;
    }

    // No-op callback (page indicator button)
    if action == "noop" {
        bot.api.answer_callback_query(cb_id, None).await?;
        return Ok(());
    }

    // Pagination callback: dg:cpage (list pagination)
    if action == "cpage" {
        bot.api.answer_callback_query(cb_id, None).await?;
        let page: usize = parts.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
        let status_filter = parts.get(3).copied().unwrap_or("");
        return super::commands::edit_simplelist_page(
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
        let session = bot.take_act_session(&chat_id).await;
        if let Some(s) = session {
            if s.item_id != cb_item_id {
                // Stale button from a previous act flow — restore the current
                // session pointing at the prompt the user actually saw.
                let prompt_mid = s.prompt.prompt_message_id;
                bot.open_act_session(&chat_id, s.item_id, &s.project, prompt_mid)
                    .await;
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
        "show" => cb_show(bot, cb_id, &chat_id, message_id, id).await,
        "read" => cb_read(bot, cb_id, &chat_id, id).await,
        "next" => cb_next(bot, cb_id, &chat_id, message_id, id).await,
        "save" => {
            bot.api
                .answer_callback_query(cb_id, Some("Saving\u{2026}"))
                .await?;
            bot.gw()
                .patch_scout_items_by_id(
                    &api_types::ScoutItemIdParams { id },
                    &api_types::ScoutLifecycleCommandRequest {
                        command: api_types::ScoutItemLifecycleCommand::Save,
                    },
                )
                .await?;
            swipe_next(bot, &chat_id, message_id, id).await
        }
        "archive" => {
            bot.api
                .answer_callback_query(cb_id, Some("Archiving\u{2026}"))
                .await?;
            bot.gw()
                .patch_scout_items_by_id(
                    &api_types::ScoutItemIdParams { id },
                    &api_types::ScoutLifecycleCommandRequest {
                        command: api_types::ScoutItemLifecycleCommand::Archive,
                    },
                )
                .await?;
            swipe_next(bot, &chat_id, message_id, id).await
        }
        "rm" => {
            bot.api
                .answer_callback_query(cb_id, Some("Removing\u{2026}"))
                .await?;
            bot.gw()
                .delete_scout_items_by_id(&api_types::ScoutItemIdParams { id })
                .await?;
            swipe_next(bot, &chat_id, message_id, id).await
        }
        "process" => {
            bot.api
                .answer_callback_query(cb_id, Some("Reprocessing\u{2026}"))
                .await?;
            bot.gw()
                .post_scout_process(&api_types::ScoutProcessRequest { id: Some(id) })
                .await?;
            bot.api
                .edit_message_text(
                    &chat_id,
                    message_id,
                    "\u{1f504} Reprocessing\u{2026}",
                    None,
                    None,
                )
                .await?;
            Ok(())
        }
        "ask" => {
            bot.api
                .answer_callback_query(cb_id, Some("Q&A started"))
                .await?;
            let title = {
                let item = bot
                    .gw()
                    .get_scout_items_by_id(&api_types::ScoutItemIdParams { id })
                    .await?;
                item.title.unwrap_or_else(|| "item".to_string())
            };
            // Send the prompt first so we can capture its message_id, then
            // register the QA session against that prompt.
            let sent = bot
                .api
                .send_message(
                    &chat_id,
                    &format!(
                        "\u{1f4ac} Q&A session for <b>#{id}</b>: {}\n\nType your question.",
                        crate::telegram_format::escape_html(&title),
                    ),
                    Some("HTML"),
                    Some(super::formatting::qa_session_kb(id)),
                    true,
                )
                .await?;
            let prompt_mid = sent.get("message_id").and_then(|v| v.as_i64()).unwrap_or(0);
            bot.open_qa_session(&chat_id, id, prompt_mid).await;
            Ok(())
        }
        "act" => act::cb_act(bot, cb_id, &chat_id, id, &config).await,
        "endqa" => {
            bot.close_qa_session(&chat_id).await;
            bot.api
                .answer_callback_query(cb_id, Some("Session ended"))
                .await?;
            let item = bot
                .gw()
                .get_scout_items_by_id(&api_types::ScoutItemIdParams { id })
                .await?;
            let item_val = serde_json::to_value(&item).unwrap_or_default();
            let summary = item.summary.as_deref();
            let text = format_swipe_card(&item_val, summary);
            let tg_url = item.telegraph_url.as_deref();
            let kb = swipe_card_kb(id, tg_url);
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
) -> Result<()> {
    bot.api.answer_callback_query(cb_id, None).await?;
    let item = bot
        .gw()
        .get_scout_items_by_id(&api_types::ScoutItemIdParams { id })
        .await?;
    let item_val = serde_json::to_value(&item).unwrap_or_default();
    let summary = item.summary.as_deref();
    let text = format_swipe_card(&item_val, summary);
    let tg_url = item.telegraph_url.as_deref();
    let kb = swipe_card_kb(id, tg_url);
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

/// Read article — always open the extracted Telegraph article.
async fn cb_read(bot: &TelegramBot, cb_id: &str, chat_id: &str, id: i64) -> Result<()> {
    bot.api
        .answer_callback_query(cb_id, Some("Loading\u{2026}"))
        .await?;

    let item = bot
        .gw()
        .get_scout_items_by_id(&api_types::ScoutItemIdParams { id })
        .await?;
    let title = item.title.as_deref().unwrap_or("Untitled").to_string();
    let article = bot
        .gw()
        .get_scout_items_by_id_article(&api_types::ScoutItemIdParams { id })
        .await?;

    let telegraph_url = match article.telegraph_url {
        Some(url) => url,
        None => {
            // No cached URL — publish now via gateway
            let result = bot
                .gw()
                .post_scout_items_by_id_telegraph(&api_types::ScoutItemIdParams { id })
                .await?;
            result.url
        }
    };

    let kb = telegraph_read_kb(id, &telegraph_url);
    let msg = format!(
        "\u{1f4d6} <a href=\"{}\">{}</a>",
        crate::telegram_format::escape_html(&telegraph_url),
        crate::telegram_format::escape_html(&title),
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
) -> Result<()> {
    bot.api.answer_callback_query(cb_id, None).await?;
    swipe_next(bot, chat_id, message_id, current_id).await
}

/// Advance to the next processed item after the given ID.
///
/// Edits the current message in place when possible.
async fn swipe_next(
    bot: &TelegramBot,
    chat_id: &str,
    message_id: i64,
    after_id: i64,
) -> Result<()> {
    let result = bot
        .gw()
        .get_scout_items(&api_types::ScoutQuery {
            status: Some(api_types::ScoutItemStatusFilter::Processed),
            q: None,
            item_type: None,
            page: None,
            per_page: Some(10_000),
        })
        .await?;

    // Items are DESC (newest first); "next" = next lower ID
    let next_id = result
        .items
        .iter()
        .find(|item| item.id < after_id)
        .map(|item| item.id);

    match next_id {
        Some(id) => {
            let full_item = bot
                .gw()
                .get_scout_items_by_id(&api_types::ScoutItemIdParams { id })
                .await?;
            let full_item_val = serde_json::to_value(&full_item).unwrap_or_default();
            let summary = full_item.summary.as_deref();
            let text = format_swipe_card(&full_item_val, summary);
            let tg_url = full_item.telegraph_url.as_deref();
            let kb = swipe_card_kb(id, tg_url);

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
