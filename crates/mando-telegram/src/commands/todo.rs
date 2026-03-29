//! `/todo` command — add tasks with confirmation flow.
//!
//! Flow: parse items (multi-line) -> show confirmation with inline keyboard.
//! Confirmation callbacks are handled in `callbacks.rs`.

use crate::bot::{TelegramBot, TodoItem};
use anyhow::Result;
use serde_json::json;

/// Handle `/todo [items]`.
///
/// If no items provided, sets pending state so next plain-text message
/// is treated as todo input. If items given, parses and shows confirmation.
pub async fn handle(bot: &mut TelegramBot, chat_id: &str, args: &str) -> Result<()> {
    if args.trim().is_empty() {
        bot.set_pending_todo(chat_id);
        bot.send_html(
            chat_id,
            "Type your todo item(s) below (multi-line supported).\nAny other command cancels.",
        )
        .await?;
        return Ok(());
    }

    bot.clear_pending_todo(chat_id);
    execute_todo(bot, chat_id, args).await
}

/// Process todo text — parse items and show confirmation keyboard.
pub async fn execute_todo(bot: &mut TelegramBot, chat_id: &str, raw_text: &str) -> Result<()> {
    execute_todo_with_photo(bot, chat_id, raw_text, None).await
}

/// Process todo text with optional photo attachment.
pub async fn execute_todo_with_photo(
    bot: &mut TelegramBot,
    chat_id: &str,
    raw_text: &str,
    photo_file_id: Option<String>,
) -> Result<()> {
    // Parse items from raw text (each non-empty line is an item)
    let items: Vec<&str> = raw_text
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect();

    if items.is_empty() {
        bot.send_html(chat_id, "\u{26a0}\u{fe0f} No items extracted.")
            .await?;
        return Ok(());
    }

    // Detect project prefix for each item
    let projects = bot.config().read().await.captain.projects.clone();
    let single_project = if projects.len() == 1 {
        projects.values().next().map(|pc| pc.name.clone())
    } else {
        None
    };

    let action_id = super::short_uuid();
    let mut todo_items = Vec::new();
    let mut lines = Vec::new();
    for (i, item) in items.iter().enumerate() {
        let (matched_slug, cleaned) = mando_config::match_project_by_prefix(item, &projects);
        let project = matched_slug.clone().or_else(|| single_project.clone());
        let repo_tag = if let Some(ref name) = project {
            format!(" \u{2192} <b>{}</b>", mando_shared::escape_html(name))
        } else {
            String::new()
        };
        let title = if matched_slug.is_some() {
            cleaned.to_string()
        } else {
            item.to_string()
        };
        let display = mando_shared::escape_html(&title);
        let photo_tag = if i == 0 && photo_file_id.is_some() {
            " \u{1f4ce}"
        } else {
            ""
        };
        lines.push(format!("{}. {}{}{}", i + 1, display, repo_tag, photo_tag));
        let photo = if i == 0 { photo_file_id.clone() } else { None };
        todo_items.push(TodoItem {
            title,
            project,
            photo_file_id: photo,
        });
    }
    let mut preview_lines = vec![format!(
        "\u{2705} Parsed {} item(s). Review below:\n",
        todo_items.len()
    )];
    preview_lines.extend(lines);
    let preview = preview_lines.join("\n");

    let needs_project = todo_items.iter().any(|i| i.project.is_none());

    let keyboard = if needs_project && projects.len() > 1 {
        // Show project buttons instead of Confirm — one tap to pick + confirm.
        let names: Vec<String> = projects
            .values()
            .map(|pc| pc.name.clone())
            .filter(|n| !n.is_empty())
            .collect();
        let project_buttons: Vec<serde_json::Value> = names
            .iter()
            .enumerate()
            .map(|(idx, name)| {
                json!({"text": name, "callback_data": format!("todo_project:{action_id}:{idx}")})
            })
            .collect();
        // Project buttons: 2 per row. Then Cancel row.
        let mut rows: Vec<Vec<serde_json::Value>> = project_buttons
            .chunks(2)
            .map(|chunk| chunk.to_vec())
            .collect();
        rows.push(vec![
            json!({"text": "Cancel", "callback_data": format!("todo_confirm:cancel:{action_id}")}),
        ]);
        bot.store_todo_confirm(&action_id, chat_id, todo_items, names);
        json!({"inline_keyboard": rows})
    } else {
        // All items have a project (prefix match or single project) — simple confirm.
        bot.store_todo_confirm(&action_id, chat_id, todo_items, vec![]);
        json!({
            "inline_keyboard": [[
                {"text": "\u{2705} Confirm", "callback_data": format!("todo_confirm:confirm:{action_id}")},
                {"text": "\u{270f}\u{fe0f} Edit", "callback_data": format!("todo_confirm:edit:{action_id}")},
                {"text": "Cancel", "callback_data": format!("todo_confirm:cancel:{action_id}")},
            ]]
        })
    };

    bot.api()
        .send_message(chat_id, &preview, Some("HTML"), Some(keyboard), true)
        .await?;

    Ok(())
}
