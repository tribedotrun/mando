//! `/ask [end|message]` — multi-turn Q&A on tasks with codebase access.

use crate::bot::TelegramBot;
use anyhow::Result;
use mando_shared::{escape_html, render_markdown_reply_html};
use serde_json::json;

/// Inline keyboard for ask sessions.
fn ask_kb() -> serde_json::Value {
    json!({
        "inline_keyboard": [[
            {"text": "End session", "callback_data": "ask:end"},
        ]]
    })
}

/// Handle `/ask [end|message]`.
pub async fn handle(bot: &mut TelegramBot, chat_id: &str, args: &str) -> Result<()> {
    let text = args.trim();

    // /ask end — close session
    if text.eq_ignore_ascii_case("end") {
        if let Some(task_id) = bot.ask_session_task_id(chat_id) {
            if let Err(e) = bot
                .gw()
                .post("/api/tasks/ask/end", &json!({"id": task_id}))
                .await
            {
                tracing::warn!(module = "telegram", task_id, error = %e, "failed to end ask session");
            }
        }
        let rounds = bot.ask_session_rounds(chat_id);
        bot.close_ask_session(chat_id);
        let summary = format!(
            "Ask session ended ({rounds} turns).\n\n\
             Use /ask to start a new session on another task."
        );
        bot.send_html(chat_id, &summary).await?;
        return Ok(());
    }

    // /ask (no args) — show status or picker
    if text.is_empty() {
        if bot.has_ask_session(chat_id) {
            let rounds = bot.ask_session_rounds(chat_id);
            bot.api()
                .send_message(
                    chat_id,
                    &format!(
                        "Ask session active ({rounds} turns).\n\
                         Reply to continue, or /ask end to close."
                    ),
                    None,
                    Some(ask_kb()),
                    true,
                )
                .await?;
            return Ok(());
        }
        return show_picker(bot, chat_id).await;
    }

    // /ask <message> — continue existing session
    if bot.has_ask_session(chat_id) {
        return ask_turn(bot, chat_id, text).await;
    }

    // No session — show picker first
    show_picker(bot, chat_id).await
}

/// Show picker of askable items.
async fn show_picker(bot: &mut TelegramBot, chat_id: &str) -> Result<()> {
    let items = match super::load_tasks_or_notify(bot, chat_id).await {
        Some(items) => items,
        None => return Ok(()),
    };
    let askable: Vec<_> = items
        .iter()
        .filter(|it| it.status.is_reopenable())
        .take(20)
        .collect();

    if askable.is_empty() {
        bot.send_html(chat_id, "No completed tasks to ask about.")
            .await?;
        return Ok(());
    }

    let action_id = super::short_uuid();
    let mut lines = vec!["Pick a completed task to ask about:".to_string()];
    let mut buttons: Vec<serde_json::Value> = Vec::new();

    for (idx, item) in askable.iter().enumerate() {
        let num = idx + 1;
        let title = escape_html(&item.title);
        lines.push(format!(" {num}. {title}"));
        buttons.push(json!([{
            "text": format!("#{num}"),
            "callback_data": format!("ask:pick:{action_id}:{idx}"),
        }]));
    }
    buttons.push(json!([{
        "text": "Cancel",
        "callback_data": format!("ask:cancel:{action_id}"),
    }]));

    bot.store_ask_picker(&action_id, chat_id, &askable);

    bot.api()
        .send_message(
            chat_id,
            &lines.join("\n"),
            None,
            Some(json!({"inline_keyboard": buttons})),
            true,
        )
        .await?;

    Ok(())
}

/// Execute one ask turn via the unified `/api/tasks/ask` endpoint.
async fn ask_turn(bot: &mut TelegramBot, chat_id: &str, text: &str) -> Result<()> {
    let task_id = match bot.ask_session_task_id(chat_id) {
        Some(id) => id,
        None => {
            bot.close_ask_session(chat_id);
            bot.send_html(chat_id, "Ask session lost — use /ask to pick a task.")
                .await?;
            return Ok(());
        }
    };

    bot.increment_ask_rounds(chat_id);

    let ack = bot.send_html(chat_id, "\u{1f914} Thinking\u{2026}").await?;
    let ack_mid = ack.get("message_id").and_then(|v| v.as_i64()).unwrap_or(0);

    let response = match bot
        .gw()
        .post("/api/tasks/ask", &json!({"id": task_id, "question": text}))
        .await
    {
        Ok(resp) => resp["answer"].as_str().unwrap_or("(no answer)").to_string(),
        Err(e) => format!("\u{274c} Ask failed: {}", escape_html(&e.to_string())),
    };

    let display = render_markdown_reply_html(&response, 4000);

    bot.edit_message_with_markup(chat_id, ack_mid, &display, Some(ask_kb()))
        .await?;

    Ok(())
}

/// Handle text messages routed to ask session.
pub async fn handle_text(bot: &mut TelegramBot, chat_id: &str, text: &str) -> Result<bool> {
    if !bot.has_ask_session(chat_id) {
        return Ok(false);
    }
    ask_turn(bot, chat_id, text).await?;
    Ok(true)
}
