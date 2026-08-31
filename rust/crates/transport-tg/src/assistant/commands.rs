//! Command handlers for the assistant bot.
//!
//! Scout-related commands (scout, list, swipe) are in the sibling
//! `scout_commands` module.

use anyhow::Result;
use serde_json::Value;

use crate::telegram_format::escape_html;

use crate::bot::TelegramBot;

// Re-export scout commands used by the dispatcher.
pub use super::scout_commands::{cmd_research, cmd_scout, execute_research};

// ── /scout_add ─────────────────────────────────────────────────────

pub async fn cmd_addlink(bot: &TelegramBot, chat_id: &str, args: &str) -> Result<()> {
    if args.trim().is_empty() {
        // Send the prompt first so we can capture its message_id; the
        // pending entry stores that id so a quote-reply routes back here.
        let prompt = send_html(
            bot,
            chat_id,
            "Send a URL below (or several URLs space-separated). Add an optional title after a single URL.\nReply to this prompt to disambiguate.",
        )
        .await?;
        let prompt_mid = prompt
            .get("message_id")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        bot.set_pending_scout_add(chat_id, prompt_mid).await;
        return Ok(());
    }

    bot.take_pending_scout_add(chat_id).await;
    execute_addlink(bot, chat_id, args).await
}

/// Add one or more URLs to Scout. Reachable from inline args and from the
/// drained `pending_scout_add` follow-up text.
pub async fn execute_addlink(bot: &TelegramBot, chat_id: &str, args: &str) -> Result<()> {
    let args = args.trim();
    if args.is_empty() {
        send_html(
            bot,
            chat_id,
            "\u{26a0}\u{fe0f} No URL provided. Try <code>/scout_add &lt;url&gt;</code>.",
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
            "\u{274c} Not a valid URL. Try <code>/scout_add &lt;url&gt; [title]</code>.",
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
    super::helpers::add_and_track(bot, chat_id, mid, url, title.as_deref()).await?;
    Ok(())
}

async fn addlink_batch(bot: &TelegramBot, chat_id: &str, urls: &[&str]) -> Result<()> {
    let sent = send_html(
        bot,
        chat_id,
        &format!("\u{23f3} Adding {} links\u{2026}", urls.len()),
    )
    .await?;
    let message_id = sent["message_id"].as_i64().unwrap_or(0);

    let mut lines = Vec::new();
    for url in urls {
        match bot
            .gw()
            .post_scout_items(&api_types::ScoutAddRequest {
                url: (*url).to_string(),
                title: None,
            })
            .await
        {
            Ok(result) => {
                let id = result.id;
                let item_type = result.item_type.as_str();
                let added = result.added;
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

pub async fn cmd_simplelist(bot: &TelegramBot, chat_id: &str, args: &str) -> Result<()> {
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
) -> Result<Option<(String, Option<api_types::TelegramReplyMarkup>)>> {
    let result = bot
        .gw()
        .get_scout_items(&api_types::ScoutQuery {
            status: Some(parse_status_filter(status_filter)?),
            q: None,
            item_type: None,
            page: None,
            per_page: Some(10_000),
        })
        .await?;
    let total = result.total;

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
    for (i, item) in result
        .items
        .iter()
        .skip(start)
        .take(COMPACT_PER_PAGE)
        .enumerate()
    {
        let id = item.id;
        page_ids.push(id);
        let pos = start + i + 1;
        let title = item.title.as_deref().unwrap_or("Untitled");
        let url = item.url.as_str();
        let scores = match (item.relevance, item.quality) {
            (Some(r), Some(q)) => format!(" R:{r}\u{00b7}Q:{q}"),
            _ => String::new(),
        };
        text.push_str(&format!(
            "<b>{pos}.</b> <a href=\"{}\">{}</a>{scores}\n",
            escape_html(url),
            escape_html(title),
        ));
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

fn parse_status_filter(value: &str) -> Result<api_types::ScoutItemStatusFilter> {
    match value.trim() {
        "" | "all" => Ok(api_types::ScoutItemStatusFilter::All),
        "pending" => Ok(api_types::ScoutItemStatusFilter::Pending),
        "fetched" => Ok(api_types::ScoutItemStatusFilter::Fetched),
        "processed" => Ok(api_types::ScoutItemStatusFilter::Processed),
        "saved" => Ok(api_types::ScoutItemStatusFilter::Saved),
        "archived" => Ok(api_types::ScoutItemStatusFilter::Archived),
        "error" => Ok(api_types::ScoutItemStatusFilter::Error),
        other => anyhow::bail!("unsupported scout status filter '{other}'"),
    }
}

/// Render a paginated compact list page.
pub async fn send_simplelist_page(
    bot: &TelegramBot,
    chat_id: &str,
    status_filter: &str,
    page: usize,
) -> Result<()> {
    match render_compact_list(bot, status_filter, page).await? {
        Some((text, Some(kb))) => {
            bot.api
                .send_message(chat_id, &text, Some("HTML"), Some(kb), true)
                .await?;
        }
        Some((text, None)) => {
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
                .edit_message_text(chat_id, message_id, &text, Some("HTML"), kb)
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

// ── Helpers ─────────────────────────────────────────────────────────

pub(crate) async fn send_html(bot: &TelegramBot, chat_id: &str, text: &str) -> Result<Value> {
    bot.send_html(chat_id, text).await
}

/// Send an error message with help hint, used when commands receive unexpected arguments.
pub(crate) async fn send_help(bot: &TelegramBot, chat_id: &str, msg: &str) -> Result<()> {
    send_html(bot, chat_id, &format!("{msg}\nSee /start for commands.")).await?;
    Ok(())
}
