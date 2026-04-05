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
pub use super::scout_commands::{cmd_research, cmd_scout, show_card};
fn parse_id_list(raw: &str) -> Vec<i64> {
    raw.split(|ch: char| ch.is_whitespace() || ch == ',')
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.trim_start_matches('#').parse::<i64>().ok())
        .collect()
}

// ── /scout_add ─────────────────────────────────────────────────────

pub async fn cmd_addlink(bot: &mut TelegramBot, chat_id: &str, args: &str) -> Result<()> {
    let args = args.trim();
    if args.is_empty() {
        send_html(
            bot,
            chat_id,
            "Usage: /scout_add &lt;url&gt; [title]\nMultiple: /scout_add &lt;url1&gt; &lt;url2&gt; ...",
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
        return addlink_batch(bot, chat_id, &urls).await;
    }

    let url = parts[0];
    if !url.starts_with("http://") && !url.starts_with("https://") {
        send_html(
            bot,
            chat_id,
            "\u{274c} Not a valid URL. Usage: /scout_add &lt;url&gt; [title]",
        )
        .await?;
        return Ok(());
    }
    let title = if parts.len() > 1 {
        Some(parts[1..].join(" "))
    } else {
        None
    };

    let sent = send_html(bot, chat_id, "\u{23f3} Adding\u{2026}").await?;
    let mid = sent["message_id"].as_i64().unwrap_or(0);
    super::helpers::add_and_track(&bot.api, bot.gw(), chat_id, mid, url, title.as_deref()).await?;
    Ok(())
}

async fn addlink_batch(bot: &mut TelegramBot, chat_id: &str, urls: &[&str]) -> Result<()> {
    let sent = send_html(
        bot,
        chat_id,
        &format!("\u{23f3} Adding {} links\u{2026}", urls.len()),
    )
    .await?;
    let message_id = sent["message_id"].as_i64().unwrap_or(0);

    let mut lines = Vec::new();
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
                } else {
                    lines.push(format!("#{id} already exists"));
                }
            }
            Err(e) => lines.push(format!("\u{274c} {}: {e}", escape_html(url))),
        }
    }

    if let Err(e) = bot
        .api
        .edit_message_text(chat_id, message_id, &lines.join("\n"), Some("HTML"), None)
        .await
    {
        tracing::warn!(module = "telegram", error = %e, "message send failed");
    }
    Ok(())
}

// ── /scout_list ───────────────────────────────────────────────────

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

/// Shared renderer for the compact (simple) list: fetches data from the gateway,
/// builds HTML text and keyboard. Returns `None` when the list is empty.
async fn render_compact_list(
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
    Ok(Some((text, kb)))
}

/// Render a paginated compact list page.
pub async fn send_simplelist_page(
    bot: &TelegramBot,
    chat_id: &str,
    status_filter: &str,
    page: usize,
) -> Result<()> {
    match render_compact_list(bot, status_filter, page).await? {
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

/// Edit a message in place with a compact list page (for callbacks).
pub async fn edit_simplelist_page(
    bot: &TelegramBot,
    chat_id: &str,
    message_id: i64,
    status_filter: &str,
    page: usize,
) -> Result<()> {
    match render_compact_list(bot, status_filter, page).await? {
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

// ── bulkstatus (removed from TG, handler kept for reuse) ──────────

pub async fn cmd_bulk_status(bot: &mut TelegramBot, chat_id: &str, args: &str) -> Result<()> {
    let mut parts = args.split_whitespace();
    let Some(status) = parts.next() else {
        send_html(
            bot,
            chat_id,
            "Usage: /bulkstatus &lt;pending|processed|saved|archived&gt; &lt;id...&gt;\nExample: /bulkstatus archived 12 14 18",
        )
        .await?;
        return Ok(());
    };

    let ids = parse_id_list(&parts.collect::<Vec<_>>().join(" "));
    if ids.is_empty() {
        send_html(bot, chat_id, "Provide at least one Scout item ID.").await?;
        return Ok(());
    }
    let count = ids.len();

    bot.gw()
        .post(
            "/api/scout/bulk",
            &serde_json::json!({"ids": ids, "updates": {"status": status}}),
        )
        .await?;

    send_html(
        bot,
        chat_id,
        &format!(
            "✅ Updated {} Scout item(s) to <b>{}</b>.",
            count,
            escape_html(status)
        ),
    )
    .await?;
    Ok(())
}

// ── bulkdelete (removed from TG, handler kept for reuse) ──────────

pub async fn cmd_bulk_delete(bot: &mut TelegramBot, chat_id: &str, args: &str) -> Result<()> {
    let ids = parse_id_list(args);
    if ids.is_empty() {
        send_html(
            bot,
            chat_id,
            "Usage: /bulkdelete &lt;id...&gt;\nExample: /bulkdelete 12 14 18",
        )
        .await?;
        return Ok(());
    }
    let count = ids.len();

    bot.gw()
        .post("/api/scout/bulk-delete", &serde_json::json!({"ids": ids}))
        .await?;

    send_html(
        bot,
        chat_id,
        &format!("🗑️ Deleted {} Scout item(s).", count),
    )
    .await?;
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
