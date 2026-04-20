//! `/todo` command -- add tasks with AI-powered parsing.
//!
//! All input (single or multi-line) goes through AI for title normalization.
//! If project can't be inferred, user picks via inline keyboard first.

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

/// Process todo text -- all input goes through AI parsing.
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
    let lines: Vec<&str> = raw_text
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect();

    if lines.is_empty() {
        bot.send_html(chat_id, "\u{26a0}\u{fe0f} No items extracted.")
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

    // --- Single line: AI-parse the title before creating ---
    if lines.len() == 1 {
        let (matched_slug, cleaned) =
            settings::config::match_project_by_prefix(lines[0], &projects);
        let project = matched_slug.or(single_project);
        let title = cleaned.to_string();

        if project.is_none() && projects.len() > 1 {
            // Need project picker for single item too.
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
            bot.store_todo_confirm(&action_id, chat_id, todo_items, names);
            bot.api()
                .send_message(
                    chat_id,
                    &format!(
                        "\u{1f4cb} <b>{}</b>\n\nPick a project:",
                        crate::telegram_format::escape_html(&title)
                    ),
                    Some("HTML"),
                    Some(keyboard),
                    true,
                )
                .await?;
            return Ok(());
        }

        // Route through AI for title normalization.
        let Some(project) = project else {
            // Picker above returns early when project is unresolved; reaching
            // this arm means no project is configured — tell the user.
            bot.api()
                .send_message(
                    chat_id,
                    "\u{274c} No project configured — run `mando project add` first.",
                    Some("Markdown"),
                    None,
                    true,
                )
                .await?;
            return Ok(());
        };
        ai_parse_and_create(bot, chat_id, &title, &project, photo_file_id).await?;
        return Ok(());
    }

    // --- Multi-line: detect project, then AI parse ---

    // Check first line for project prefix.
    let (detected_project, _) = settings::config::match_project_by_prefix(lines[0], &projects);
    let project = detected_project.or(single_project);

    if project.is_none() && projects.len() > 1 {
        // Store raw text + photo, show project picker. AI parse happens after pick.
        let action_id = super::short_uuid();
        let names: Vec<String> = projects
            .values()
            .map(|pc| pc.name.clone())
            .filter(|n| !n.is_empty())
            .collect();

        // Store the raw text as a single TodoItem — callbacks will AI-parse it.
        let todo_items = vec![TodoItem {
            title: raw_text.trim().to_string(),
            project: None,
            photo_file_id,
        }];
        let keyboard = build_project_picker(&action_id, &names);
        bot.store_todo_confirm(&action_id, chat_id, todo_items, names);

        let line_count = lines.len();
        bot.api()
            .send_message(
                chat_id,
                &format!("\u{1f4cb} {line_count} lines entered. Pick a project:"),
                Some("HTML"),
                Some(keyboard),
                true,
            )
            .await?;
        return Ok(());
    }

    // Project known — None case returns early via the picker above.
    let Some(project) = project else {
        bot.api()
            .send_message(
                chat_id,
                "\u{274c} No project configured — run `mando project add` first.",
                Some("Markdown"),
                None,
                true,
            )
            .await?;
        return Ok(());
    };
    ai_parse_and_create(bot, chat_id, raw_text, &project, photo_file_id).await
}

/// Call gateway AI endpoint to parse text, then create tasks.
pub(crate) async fn ai_parse_and_create(
    bot: &TelegramBot,
    chat_id: &str,
    raw_text: &str,
    project: &str,
    photo_file_id: Option<String>,
) -> Result<()> {
    let mid = bot
        .send_loading(chat_id, "\u{1f9e0} Parsing tasks\u{2026}")
        .await?;

    let body = json!({
        "text": raw_text,
        "project": project,
    });
    let result = bot
        .gw()
        .post_typed::<_, api_types::ParseTodosResponse>("/api/ai/parse-todos", &body)
        .await;

    let parsed_items: Vec<String> = match result {
        Ok(resp) => {
            if resp.items.is_empty() {
                bot.edit_message(chat_id, mid, "\u{26a0}\u{fe0f} AI returned no tasks.")
                    .await?;
                return Ok(());
            }
            resp.items
        }
        Err(e) => {
            bot.edit_message(
                chat_id,
                mid,
                &format!(
                    "\u{26a0}\u{fe0f} Failed to parse: {}",
                    crate::telegram_format::escape_html(&e.to_string())
                ),
            )
            .await?;
            return Ok(());
        }
    };

    let todo_items: Vec<TodoItem> = parsed_items
        .into_iter()
        .enumerate()
        .map(|(i, title)| TodoItem {
            title,
            project: Some(project.to_string()),
            photo_file_id: if i == 0 { photo_file_id.clone() } else { None },
        })
        .collect();

    crate::callback_actions::add_todo_items(bot, chat_id, &todo_items, Some(mid)).await
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
