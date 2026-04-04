//! Scout-related command handlers for the assistant bot.

use anyhow::Result;
use serde_json::Value;

use mando_shared::telegram_format::escape_html;

use super::commands::send_html;
use super::formatting::{format_swipe_card, list_kb, render_summary_preview, swipe_card_kb};
use crate::bot::TelegramBot;
use crate::gateway_paths as paths;

/// Items per page for paginated list views.
const ITEMS_PER_PAGE: usize = 3;

// ── /scout_list (rich list with summaries) ────────────────────────

pub async fn cmd_list(bot: &mut TelegramBot, chat_id: &str, args: &str) -> Result<()> {
    if let Err(e) = send_list_page(bot, chat_id, args.trim(), 0).await {
        send_html(
            bot,
            chat_id,
            &format!("\u{274c} Failed to load: {}", escape_html(&e.to_string())),
        )
        .await?;
    }
    Ok(())
}

/// Render the summary list text + keyboard for a given page. Shared by send/edit.
async fn render_summary_list(
    bot: &TelegramBot,
    status_filter: &str,
    page: usize,
) -> Result<Option<(String, Value)>> {
    let result = bot
        .gw()
        .get(&paths::scout_items_with_status(Some(status_filter), 10000))
        .await?;
    let items = result["items"].as_array();
    let total = result["total"].as_u64().map(|t| t as usize).unwrap_or(0);

    if total == 0 {
        return Ok(None);
    }

    let total_pages = total.div_ceil(ITEMS_PER_PAGE);
    let page = page.min(total_pages.saturating_sub(1));
    let start = page * ITEMS_PER_PAGE;
    let status_label = if status_filter.is_empty() {
        "all"
    } else {
        status_filter
    };

    let mut text = format!(
        "\u{1f4f0} <b>Scout</b> \u{2014} {} ({total} items, page {}/{})\n",
        escape_html(status_label),
        page + 1,
        total_pages,
    );

    let mut page_ids = Vec::new();
    if let Some(items) = items {
        for (i, item) in items.iter().skip(start).take(ITEMS_PER_PAGE).enumerate() {
            let id = item["id"].as_i64().unwrap_or(0);
            page_ids.push(id);
            let pos = start + i + 1;
            let title = item["title"].as_str().unwrap_or("Untitled");
            let url = item["url"].as_str().unwrap_or("");
            let scores = match (item["relevance"].as_i64(), item["quality"].as_i64()) {
                (Some(r), Some(q)) => format!(" R:{r}\u{00b7}Q:{q}"),
                _ => String::new(),
            };
            text.push_str(&format!(
                "\n<b>{pos}.</b> <a href=\"{}\">{}</a>{scores}\n",
                escape_html(url),
                escape_html(title),
            ));
            if let Ok(full) = bot.gw().get(&paths::scout_item(id)).await {
                if let Some(summary) = full["summary"].as_str() {
                    let preview = render_summary_preview(summary);
                    if !preview.is_empty() {
                        text.push_str(&preview);
                        text.push('\n');
                    }
                }
            }
        }
    }

    let kb = list_kb(
        &page_ids,
        page,
        total_pages,
        status_label,
        "dg:page",
        3,
        start,
    );
    Ok(Some((text, kb)))
}

/// Render a paginated list page (used by both command and callback).
pub async fn send_list_page(
    bot: &TelegramBot,
    chat_id: &str,
    status_filter: &str,
    page: usize,
) -> Result<()> {
    match render_summary_list(bot, status_filter, page).await? {
        Some((text, kb)) if !kb.is_null() => {
            bot.api
                .send_message(chat_id, &text, Some("HTML"), Some(kb), true)
                .await?;
        }
        Some((text, _)) => {
            send_html(bot, chat_id, &text).await?;
        }
        None => {
            let msg = if status_filter.is_empty() {
                "\u{1f4f0} No scout items.".into()
            } else {
                format!(
                    "\u{1f4f0} No items with status <b>{}</b>.",
                    escape_html(status_filter)
                )
            };
            send_html(bot, chat_id, &msg).await?;
        }
    }
    Ok(())
}

/// Edit a message in place with a paginated list page (for callbacks).
pub async fn edit_list_page(
    bot: &TelegramBot,
    chat_id: &str,
    message_id: i64,
    status_filter: &str,
    page: usize,
) -> Result<()> {
    match render_summary_list(bot, status_filter, page).await? {
        Some((text, kb)) => {
            if let Err(e) = bot
                .api
                .edit_message_text(
                    chat_id,
                    message_id,
                    &text,
                    Some("HTML"),
                    if kb.is_null() { None } else { Some(kb) },
                )
                .await
            {
                tracing::warn!(module = "telegram", error = %e, "message send failed");
            }
        }
        None => {
            let msg = if status_filter.is_empty() {
                "\u{1f4f0} No scout items.".into()
            } else {
                format!(
                    "\u{1f4f0} No items with status <b>{}</b>.",
                    escape_html(status_filter)
                )
            };
            if let Err(e) = bot
                .api
                .edit_message_text(chat_id, message_id, &msg, Some("HTML"), None)
                .await
            {
                tracing::warn!(module = "telegram", error = %e, "message send failed");
            }
        }
    }
    Ok(())
}

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
            let processed = result["processed"].as_u64().unwrap_or(0);
            let links = result["links"].as_array();
            let mut text =
                format!("\u{2705} Research complete: {added} added, {processed} processed\n");
            if let Some(links) = links {
                for link in links.iter().take(10) {
                    let title = link["title"].as_str().unwrap_or("Untitled");
                    let url = link["url"].as_str().unwrap_or("");
                    text.push_str(&format!(
                        "\n\u{2022} <a href=\"{}\">{}</a>",
                        escape_html(url),
                        escape_html(title),
                    ));
                }
            }
            if message_id > 0 {
                if let Err(e) = bot
                    .api
                    .edit_message_text(chat_id, message_id, &text, Some("HTML"), None)
                    .await
                {
                    tracing::warn!(module = "telegram", error = %e, "message send failed");
                }
            }
        }
        Err(e) => {
            if message_id > 0 {
                if let Err(e) = bot
                    .api
                    .edit_message_text(
                        chat_id,
                        message_id,
                        &format!("\u{274c} Research failed: {}", escape_html(&e.to_string())),
                        Some("HTML"),
                        None,
                    )
                    .await
                {
                    tracing::warn!(module = "telegram", error = %e, "message send failed");
                }
            }
        }
    }
    Ok(())
}
