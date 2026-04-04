//! `/todo` command — add tasks with AI-powered bulk parsing.
//!
//! Single line: create one task directly (fast path).
//! Multi-line: AI parses into individual tasks. All tasks share one project.
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

/// Process todo text — single line goes direct, multi-line uses AI parsing.
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
    let single_project = if projects.len() == 1 {
        projects.values().next().map(|pc| pc.name.clone())
    } else {
        None
    };

    // --- Single line: fast path, create directly ---
    if lines.len() == 1 {
        let (matched_slug, cleaned) = mando_config::match_project_by_prefix(lines[0], &projects);
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
                        mando_shared::escape_html(&title)
                    ),
                    Some("HTML"),
                    Some(keyboard),
                    true,
                )
                .await?;
            return Ok(());
        }

        let items = vec![TodoItem {
            title,
            project,
            photo_file_id,
        }];
        crate::callback_actions::add_todo_items(bot, chat_id, &items).await?;
        return Ok(());
    }

    // --- Multi-line: detect project, then AI parse ---

    // Check first line for project prefix.
    let (detected_project, _) = mando_config::match_project_by_prefix(lines[0], &projects);
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

    // Project known — AI parse and create.
    ai_parse_and_create(bot, chat_id, raw_text, project.as_deref(), photo_file_id).await
}

/// Call gateway AI endpoint to parse text, then create tasks.
pub(crate) async fn ai_parse_and_create(
    bot: &TelegramBot,
    chat_id: &str,
    raw_text: &str,
    project: Option<&str>,
    photo_file_id: Option<String>,
) -> Result<()> {
    bot.send_html(chat_id, "\u{1f9e0} Parsing tasks\u{2026}")
        .await?;

    let body = json!({
        "text": raw_text,
        "project": project,
    });
    let result = bot.gw().post("/api/ai/parse-todos", &body).await;

    let parsed_items: Vec<String> = match result {
        Ok(resp) => {
            let items: Vec<String> = resp["items"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            if items.is_empty() {
                bot.send_html(chat_id, "\u{26a0}\u{fe0f} AI returned no tasks.")
                    .await?;
                return Ok(());
            }
            items
        }
        Err(e) => {
            bot.send_html(
                chat_id,
                &format!(
                    "\u{26a0}\u{fe0f} Failed to parse: {}",
                    mando_shared::escape_html(&e.to_string())
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
            project: project.map(String::from),
            photo_file_id: if i == 0 { photo_file_id.clone() } else { None },
        })
        .collect();

    crate::callback_actions::add_todo_items(bot, chat_id, &todo_items).await
}

fn build_project_picker(action_id: &str, names: &[String]) -> serde_json::Value {
    let buttons: Vec<serde_json::Value> = names
        .iter()
        .enumerate()
        .map(|(idx, name)| {
            json!({"text": name, "callback_data": format!("todo_project:{action_id}:{idx}")})
        })
        .collect();
    let mut rows: Vec<Vec<serde_json::Value>> = buttons.chunks(2).map(|c| c.to_vec()).collect();
    rows.push(vec![
        json!({"text": "Cancel", "callback_data": format!("todo_confirm:cancel:{action_id}")}),
    ]);
    json!({"inline_keyboard": rows})
}
