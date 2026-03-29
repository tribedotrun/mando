//! Command handlers for the assistant bot.
//!
//! Scout-related commands (scout, list, swipe) are in the sibling
//! `scout_commands` module.

use anyhow::Result;
use serde_json::Value;

use mando_shared::telegram_format::escape_html;

use crate::bot::TelegramBot;
use crate::gateway_paths as paths;

// Re-export scout commands used by the dispatcher.
pub use super::scout_commands::{
    auto_process_batch, auto_process_single, cmd_list, cmd_research, cmd_scout, edit_list_page,
    show_card,
};

// ── /addlink ────────────────────────────────────────────────────────

pub async fn cmd_addlink(
    bot: &mut TelegramBot,
    chat_id: &str,
    args: &str,
    is_group: bool,
) -> Result<()> {
    let args = args.trim();
    if args.is_empty() {
        send_html(
            bot,
            chat_id,
            "Usage: /addlink <url> [title]\nMultiple: /addlink <url1> <url2> ...",
        )
        .await?;
        return Ok(());
    }

    let parts: Vec<&str> = args.split_whitespace().collect();
    let urls: Vec<&str> = parts
        .iter()
        .filter(|p| p.starts_with("http://") || p.starts_with("https://"))
        .copied()
        .collect();

    if urls.len() > 1 && urls.len() == parts.len() {
        return addlink_batch(bot, chat_id, &urls, is_group).await;
    }

    let url = parts[0];
    if !url.starts_with("http://") && !url.starts_with("https://") {
        send_html(
            bot,
            chat_id,
            "\u{274c} Not a valid URL. Usage: /addlink <url> [title]",
        )
        .await?;
        return Ok(());
    }
    let title = if parts.len() > 1 {
        Some(parts[1..].join(" "))
    } else {
        None
    };

    let sent = send_html(bot, chat_id, "\u{1f4e5} Adding\u{2026}").await?;
    let message_id = sent["message_id"].as_i64().unwrap_or(0);
    let body = serde_json::json!({"url": url, "title": title});
    let result = match bot.gw().post(paths::SCOUT_ITEMS, &body).await {
        Ok(r) => r,
        Err(e) => {
            if message_id > 0 {
                let _ = bot
                    .api
                    .edit_message_text(
                        chat_id,
                        message_id,
                        &format!("\u{274c} Failed to add: {}", escape_html(&e.to_string())),
                        Some("HTML"),
                        None,
                    )
                    .await;
            }
            return Ok(());
        }
    };
    let id = result["id"].as_i64().unwrap_or(0);
    let added = result["added"].as_bool().unwrap_or(false);
    let item_type = result["type"].as_str().unwrap_or("unknown");

    if added {
        let msg = format!(
            "\u{1f4e5} Added #{id}: <a href=\"{}\">{item_type}</a>",
            escape_html(url),
        );
        if message_id > 0 {
            let _ = bot
                .api
                .edit_message_text(chat_id, message_id, &msg, Some("HTML"), None)
                .await;
            auto_process_single(bot, chat_id, message_id, id, is_group).await;
        }
    } else {
        if message_id > 0 {
            let _ = bot
                .api
                .edit_message_text(
                    chat_id,
                    message_id,
                    &format!(
                        "Already exists as #{id} (<a href=\"{}\">{item_type}</a>)",
                        escape_html(url),
                    ),
                    Some("HTML"),
                    None,
                )
                .await;
        }
    }
    Ok(())
}

async fn addlink_batch(
    bot: &mut TelegramBot,
    chat_id: &str,
    urls: &[&str],
    is_group: bool,
) -> Result<()> {
    let sent = send_html(
        bot,
        chat_id,
        &format!("\u{23f3} Adding {} links\u{2026}", urls.len()),
    )
    .await?;
    let message_id = sent["message_id"].as_i64().unwrap_or(0);

    let mut lines = Vec::new();
    let mut added_ids = Vec::new();
    for url in urls {
        let body = serde_json::json!({"url": url});
        match bot.gw().post(paths::SCOUT_ITEMS, &body).await {
            Ok(result) => {
                let id = result["id"].as_i64().unwrap_or(0);
                let item_type = result["type"].as_str().unwrap_or("unknown");
                let added = result["added"].as_bool().unwrap_or(false);
                if added {
                    lines.push(format!(
                        "\u{1f4e5} #{id}: <a href=\"{}\">{item_type}</a>",
                        escape_html(url),
                    ));
                    added_ids.push(id);
                } else {
                    lines.push(format!("#{id} already exists"));
                }
            }
            Err(e) => lines.push(format!("\u{274c} {}: {e}", escape_html(url))),
        }
    }

    if added_ids.is_empty() {
        // Nothing new — just show the status
        let _ = bot
            .api
            .edit_message_text(chat_id, message_id, &lines.join("\n"), Some("HTML"), None)
            .await;
    } else {
        // Edit to show what was added, then process in place
        let _ = bot
            .api
            .edit_message_text(chat_id, message_id, &lines.join("\n"), Some("HTML"), None)
            .await;
        auto_process_batch(bot, chat_id, message_id, &added_ids, is_group).await;
    }
    Ok(())
}

// ── /simplelist ─────────────────────────────────────────────────────

pub async fn cmd_simplelist(bot: &mut TelegramBot, chat_id: &str, args: &str) -> Result<()> {
    if let Err(e) = send_simplelist_page(bot, chat_id, args.trim(), 0).await {
        send_html(
            bot,
            chat_id,
            &format!("\u{274c} Failed to load: {}", escape_html(&e.to_string())),
        )
        .await?;
    }
    Ok(())
}

/// Items per page for compact list.
const COMPACT_PER_PAGE: usize = 10;

/// Render a paginated compact list page.
pub async fn send_simplelist_page(
    bot: &TelegramBot,
    chat_id: &str,
    status_filter: &str,
    page: usize,
) -> Result<()> {
    let result = bot
        .gw()
        .get(&paths::scout_items_with_status(Some(status_filter), 10000))
        .await?;
    let items = result["items"].as_array();
    let total = result["total"].as_u64().map(|t| t as usize).unwrap_or(0);

    if total == 0 {
        let msg = if status_filter.is_empty() {
            "\u{1f4f0} No scout items.".into()
        } else {
            format!(
                "\u{1f4f0} No items with status <b>{}</b>.",
                escape_html(status_filter)
            )
        };
        send_html(bot, chat_id, &msg).await?;
        return Ok(());
    }

    let total_pages = total.div_ceil(COMPACT_PER_PAGE);
    let page = page.min(total_pages.saturating_sub(1));
    let start = page * COMPACT_PER_PAGE;
    let status_label = if status_filter.is_empty() {
        "all"
    } else {
        status_filter
    };
    let mut text = format!(
        "\u{1f4f0} <b>Scout</b> \u{2014} {} ({total} items)\n",
        escape_html(status_label),
    );

    let mut page_ids = Vec::new();
    if let Some(items) = items {
        for (i, item) in items.iter().skip(start).take(COMPACT_PER_PAGE).enumerate() {
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
                "<b>{pos}.</b> <a href=\"{}\">{}</a>{scores}\n",
                escape_html(url),
                escape_html(title),
            ));
        }
    }

    let kb = super::formatting::list_kb(
        &page_ids,
        page,
        total_pages,
        status_label,
        "dg:cpage",
        5,
        start,
    );
    if kb.is_null() {
        send_html(bot, chat_id, &text).await?;
    } else {
        bot.api
            .send_message(chat_id, &text, Some("HTML"), Some(kb), true)
            .await?;
    }
    Ok(())
}

/// Edit a message in place with a compact list page (for callbacks).
pub async fn edit_simplelist_page(
    bot: &TelegramBot,
    chat_id: &str,
    message_id: i64,
    status_filter: &str,
    page: usize,
) -> Result<()> {
    let result = bot
        .gw()
        .get(&paths::scout_items_with_status(Some(status_filter), 10000))
        .await?;
    let items = result["items"].as_array();
    let total = result["total"].as_u64().map(|t| t as usize).unwrap_or(0);
    let total_pages = total.div_ceil(COMPACT_PER_PAGE);
    let page = page.min(total_pages.saturating_sub(1));
    let start = page * COMPACT_PER_PAGE;
    let status_label = if status_filter.is_empty() {
        "all"
    } else {
        status_filter
    };

    let mut text = format!(
        "\u{1f4f0} <b>Scout</b> \u{2014} {} ({total} items)\n",
        escape_html(status_label),
    );

    let mut page_ids = Vec::new();
    if let Some(items) = items {
        for (i, item) in items.iter().skip(start).take(COMPACT_PER_PAGE).enumerate() {
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
                "<b>{pos}.</b> <a href=\"{}\">{}</a>{scores}\n",
                escape_html(url),
                escape_html(title),
            ));
        }
    }

    let kb = super::formatting::list_kb(
        &page_ids,
        page,
        total_pages,
        status_label,
        "dg:cpage",
        5,
        start,
    );
    let _ = bot
        .api
        .edit_message_text(
            chat_id,
            message_id,
            &text,
            Some("HTML"),
            if kb.is_null() { None } else { Some(kb) },
        )
        .await;
    Ok(())
}

// ── Helpers ─────────────────────────────────────────────────────────

pub(crate) async fn send_html(bot: &TelegramBot, chat_id: &str, text: &str) -> Result<Value> {
    bot.send_html(chat_id, text).await
}

/// Send an error message with help hint, used when commands receive unexpected arguments.
pub(crate) async fn send_help(bot: &TelegramBot, chat_id: &str, msg: &str) -> Result<()> {
    send_html(bot, chat_id, &format!("{msg}\nSee /start for commands.")).await?;
    Ok(())
}
