//! `/todo` command -- add one task.
//!
//! One message is one task: the text is used verbatim, never split. If the
//! project can't be inferred from a prefix or a single configured project,
//! the user picks it via an inline keyboard first.

use crate::bot::{TelegramBot, TodoItem};
use anyhow::Result;

/// Handle `/todo [text]`.
///
/// If no text is provided, sets pending state so the next plain-text message
/// is treated as todo input.
pub async fn handle(bot: &TelegramBot, chat_id: &str, args: &str) -> Result<()> {
    if args.trim().is_empty() {
        // Send the prompt first so we can capture its message_id; the
        // pending entry stores that id so a quote-reply routes back here.
        let prompt = bot
            .send_html(
                chat_id,
                "Type your todo below.\nReply to this prompt to disambiguate.",
            )
            .await?;
        let prompt_mid = prompt
            .get("message_id")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        bot.set_pending_todo(chat_id, prompt_mid).await;
        return Ok(());
    }

    bot.take_pending_todo(chat_id).await;
    execute_todo(bot, chat_id, args).await
}

/// Process todo text into exactly one task.
pub async fn execute_todo(bot: &TelegramBot, chat_id: &str, raw_text: &str) -> Result<()> {
    execute_todo_with_photo(bot, chat_id, raw_text, None).await
}

/// Process todo text with an optional photo attachment.
pub async fn execute_todo_with_photo(
    bot: &TelegramBot,
    chat_id: &str,
    raw_text: &str,
    photo_file_id: Option<String>,
) -> Result<()> {
    let text = raw_text.trim();
    if text.is_empty() {
        bot.send_html(chat_id, "\u{26a0}\u{fe0f} Nothing to add.")
            .await?;
        return Ok(());
    }

    let projects = bot.config().read().await.captain.projects.clone();
    if projects.is_empty() {
        bot.send_html(
            chat_id,
            "\u{26a0}\u{fe0f} No projects configured. Add a project first.",
        )
        .await?;
        return Ok(());
    }
    let single_project = if projects.len() == 1 {
        projects.values().next().map(|pc| pc.name.clone())
    } else {
        None
    };

    // A `<project> ...` prefix on the first line names the project and is
    // stripped from the title; otherwise the text is used verbatim.
    let (matched_slug, cleaned) = settings::match_project_by_prefix(text, &projects);
    let title = cleaned.trim().to_string();
    let project = matched_slug.or(single_project);

    let Some(project) = project else {
        let action_id = super::short_uuid();
        let names: Vec<String> = projects
            .values()
            .map(|pc| pc.name.clone())
            .filter(|n| !n.is_empty())
            .collect();
        let todo_items = vec![TodoItem {
            title: title.clone(),
            project: None,
            photo_file_id,
        }];
        let keyboard = build_project_picker(&action_id, &names);
        bot.store_todo_confirm(&action_id, chat_id, todo_items, names)
            .await;
        bot.api()
            .send_message(
                chat_id,
                &format!(
                    "\u{1f4cb} <b>{}</b>\n\nPick a project:",
                    crate::telegram_format::escape_html(&first_line(&title))
                ),
                Some("HTML"),
                Some(keyboard),
                true,
            )
            .await?;
        return Ok(());
    };

    let items = vec![TodoItem {
        title,
        project: Some(project),
        photo_file_id,
    }];
    crate::callback_actions::add_todo_items(bot, chat_id, &items, None).await
}

/// A one-line label for a task whose title may span several lines.
pub(crate) fn first_line(title: &str) -> String {
    title.lines().next().unwrap_or("").trim().to_string()
}

fn build_project_picker(action_id: &str, names: &[String]) -> api_types::TelegramReplyMarkup {
    use api_types::InlineKeyboardButton;
    let buttons: Vec<InlineKeyboardButton> = names
        .iter()
        .enumerate()
        .map(|(idx, name)| InlineKeyboardButton {
            text: name.clone(),
            callback_data: Some(format!("todo_project:{action_id}:{idx}")),
            url: None,
        })
        .collect();
    let mut rows: Vec<Vec<InlineKeyboardButton>> =
        buttons.chunks(2).map(|chunk| chunk.to_vec()).collect();
    rows.push(vec![InlineKeyboardButton {
        text: "Cancel".into(),
        callback_data: Some(format!("todo_confirm:cancel:{action_id}")),
        url: None,
    }]);
    api_types::TelegramReplyMarkup::InlineKeyboard { rows }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_line_labels_a_multi_line_title() {
        assert_eq!(first_line("Fix login\nmore detail"), "Fix login");
        assert_eq!(first_line("  Fix login  "), "Fix login");
        assert_eq!(first_line(""), "");
    }
}
