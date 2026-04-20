//! Inline keyboard callback handlers.
//!
//! Dispatch by prefix: `{prefix}:{action}:{params}`.

use anyhow::Result;
use serde_json::Value;
use tracing::debug;

use crate::bot::TelegramBot;
use crate::callbacks_picker;

/// Handle an incoming callback query.
pub async fn handle_callback(bot: &mut TelegramBot, callback: &Value) -> Result<()> {
    let data = callback.get("data").and_then(|d| d.as_str()).unwrap_or("");
    let cb_id = callback.get("id").and_then(|v| v.as_str()).unwrap_or("");
    let chat_id = callback
        .get("message")
        .and_then(|m| m.get("chat"))
        .and_then(|c| c.get("id"))
        .and_then(|v| v.as_i64())
        .map(|id| id.to_string())
        .unwrap_or_default();
    let mid = callback
        .get("message")
        .and_then(|m| m.get("message_id"))
        .and_then(|v| v.as_i64())
        .unwrap_or(0);

    debug!("Callback: data={data} chat={chat_id}");
    let parts: Vec<&str> = data.split(':').collect();
    let prefix = parts.first().copied().unwrap_or("");

    match prefix {
        "todo_confirm" => handle_todo_confirm(bot, &parts, cb_id, &chat_id, mid).await,
        "todo_project" => handle_todo_project(bot, &parts, cb_id, &chat_id, mid).await,
        "act" => callbacks_picker::handle_action_callback(bot, &parts, cb_id, &chat_id, mid).await,
        "merge" => handle_action(bot, "Merge", &parts, cb_id, &chat_id).await,
        "accept" => handle_action(bot, "Accept", &parts, cb_id, &chat_id).await,
        "view" => handle_view_cb(bot, &parts, cb_id, &chat_id, mid).await,
        "dtl" => handle_detail_cb(bot, &parts, cb_id, &chat_id, mid).await,
        "dg" => crate::assistant::callbacks::handle_callback(bot, callback).await,
        other => {
            tracing::warn!(
                module = "transport-tg-callbacks",
                action = other,
                "unrecognized callback prefix"
            );
            bot.api().answer_callback_query(cb_id, None).await?;
            Ok(())
        }
    }
}

// ── Todo confirm ─────────────────────────────────────────────────────

async fn handle_todo_confirm(
    bot: &mut TelegramBot,
    parts: &[&str],
    cb_id: &str,
    cid: &str,
    mid: i64,
) -> Result<()> {
    let action = parts.get(1).copied().unwrap_or("");
    let aid = parts.get(2).copied().unwrap_or("");
    match action {
        "confirm" => {
            // Confirm is only shown when all items already have a project
            // (prefix match or single project). Multi-project uses todo_project directly.
            if let Some(state) = bot.take_todo_confirm(aid) {
                bot.api()
                    .answer_callback_query(cb_id, Some("Writing\u{2026}"))
                    .await?;
                if let Err(e) = bot
                    .edit_message(cid, mid, "\u{23f3} Writing to task list\u{2026}")
                    .await
                {
                    tracing::warn!(module = "telegram", error = %e, "message send failed");
                }
                crate::callback_actions::add_todo_items(bot, cid, &state.items, Some(mid)).await?;
            } else {
                bot.api()
                    .answer_callback_query(cb_id, Some("Expired"))
                    .await?;
            }
        }
        "edit" => {
            bot.api()
                .answer_callback_query(cb_id, Some("Send your edits"))
                .await?;
            if let Err(e) = bot
                .edit_message(cid, mid, "\u{270f}\u{fe0f} Send your edit instructions:")
                .await
            {
                tracing::warn!(module = "telegram", error = %e, "message send failed");
            }
        }
        "cancel" => {
            bot.take_todo_confirm(aid);
            bot.api()
                .answer_callback_query(cb_id, Some("Cancelled"))
                .await?;
            if let Err(e) = bot.edit_message(cid, mid, "\u{23ed} Cancelled").await {
                tracing::warn!(module = "telegram", error = %e, "message send failed");
            }
        }
        _ => {
            bot.api().answer_callback_query(cb_id, None).await?;
        }
    }
    Ok(())
}

// ── Todo project picker ──────────────────────────────────────────────

async fn handle_todo_project(
    bot: &mut TelegramBot,
    parts: &[&str],
    cb_id: &str,
    cid: &str,
    mid: i64,
) -> Result<()> {
    let aid = parts.get(1).copied().unwrap_or("");
    let sel = parts.get(2).copied().unwrap_or("");
    if sel == "cancel" {
        bot.take_todo_confirm(aid);
        bot.api()
            .answer_callback_query(cb_id, Some("Cancelled"))
            .await?;
        if let Err(e) = bot.edit_message(cid, mid, "\u{23ed} Cancelled").await {
            tracing::warn!(module = "telegram", error = %e, "message send failed");
        }
        return Ok(());
    }
    if let Some(state) = bot.take_todo_confirm(aid) {
        let idx: usize = sel.parse().unwrap_or(usize::MAX);
        let name = state.picker_slugs.get(idx).cloned().unwrap_or_default();
        if name.is_empty() {
            bot.api()
                .answer_callback_query(cb_id, Some("Invalid selection"))
                .await?;
            return Ok(());
        }
        bot.api().answer_callback_query(cb_id, Some(&name)).await?;
        if let Err(e) = bot
            .edit_message(
                cid,
                mid,
                &format!(
                    "\u{23f3} Adding to <b>{}</b>\u{2026}",
                    crate::telegram_format::escape_html(&name)
                ),
            )
            .await
        {
            tracing::warn!(module = "telegram", error = %e, "message send failed");
        }

        // Route all items (single or multi-line) through AI for title normalization.
        let Some(first) = state.items.first() else {
            tracing::warn!(
                module = "telegram",
                "todo_project callback: empty items state"
            );
            return Ok(());
        };
        let raw = &first.title;
        let photo = first.photo_file_id.clone();
        crate::commands::todo::ai_parse_and_create(bot, cid, raw, &name, photo).await?;
    } else {
        bot.api()
            .answer_callback_query(cb_id, Some("Expired"))
            .await?;
    }
    Ok(())
}

// ── Merge / Accept ───────────────────────────────────────────────────

async fn handle_action(
    bot: &TelegramBot,
    label: &str,
    parts: &[&str],
    cb_id: &str,
    cid: &str,
) -> Result<()> {
    let item_id = parts.get(1).copied().unwrap_or("");
    bot.api()
        .answer_callback_query(cb_id, Some(&format!("{label}\u{2026}")))
        .await?;

    // Send loading placeholder (None = merge/accept send their own)
    use crate::callback_actions;
    if label == "Merge" {
        callback_actions::merge(bot, cid, item_id, None).await?;
    } else {
        callback_actions::accept(bot, cid, item_id, None).await?;
    }
    Ok(())
}

// ── View / detail callbacks ─────────────────────────────────────────

async fn handle_view_cb(
    bot: &TelegramBot,
    parts: &[&str],
    cb_id: &str,
    cid: &str,
    mid: i64,
) -> Result<()> {
    let item_id = parts.get(1).copied().unwrap_or("");
    bot.api()
        .answer_callback_query(cb_id, Some("Loading\u{2026}"))
        .await?;
    crate::commands::detail::handle_view(bot, cid, mid, item_id).await
}

async fn handle_detail_cb(
    bot: &TelegramBot,
    parts: &[&str],
    cb_id: &str,
    cid: &str,
    mid: i64,
) -> Result<()> {
    let action = parts.get(1).copied().unwrap_or("");
    match action {
        "back" => {
            bot.api().answer_callback_query(cb_id, None).await?;
            global_infra::best_effort!(
                bot.remove_keyboard(cid, mid).await,
                "callbacks: bot.remove_keyboard(cid, mid).await"
            );
            crate::commands::status::handle(bot, cid, "").await
        }
        "tl" => {
            let item_id = parts.get(2).copied().unwrap_or("");
            bot.api()
                .answer_callback_query(cb_id, Some("Loading\u{2026}"))
                .await?;
            crate::commands::timeline::handle(bot, cid, item_id).await
        }
        _ => {
            bot.api().answer_callback_query(cb_id, None).await?;
            Ok(())
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    #[test]
    fn callback_data_parsing() {
        let data = "act:pick:abc12345:3";
        let parts: Vec<&str> = data.split(':').collect();
        assert_eq!(parts[0], "act");
        assert_eq!(parts[1], "pick");
        assert_eq!(parts[2], "abc12345");
        assert_eq!(parts[3], "3");
    }

    #[test]
    fn callback_data_action() {
        let data = "act:do:42:reopen";
        let parts: Vec<&str> = data.split(':').collect();
        assert_eq!(parts[0], "act");
        assert_eq!(parts[1], "do");
        assert_eq!(parts[2], "42");
        assert_eq!(parts[3], "reopen");
    }

    #[test]
    fn callback_prefix_dispatch() {
        let prefixes = [
            "todo_confirm",
            "todo_project",
            "act",
            "merge",
            "accept",
            "view",
            "dtl",
            "dg",
        ];
        assert_eq!(prefixes.len(), 8);
    }
}
