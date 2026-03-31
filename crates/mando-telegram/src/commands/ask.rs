//! `/ask [end|message]` — read-only Q&A on completed tasks.

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

    // /ask end — close session and re-send summary card (#357)
    if text.eq_ignore_ascii_case("end") {
        let rounds = bot.ask_session_rounds(chat_id);
        let key = format!("ask:{chat_id}");
        bot.gw()
            .post("/api/ops/end", &json!({"key": key}))
            .await
            .ok();
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

/// Show picker of completed items.
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

/// Execute one ask turn via HTTP to gateway.
async fn ask_turn(bot: &mut TelegramBot, chat_id: &str, text: &str) -> Result<()> {
    bot.increment_ask_rounds(chat_id);

    let ack = bot.send_html(chat_id, "\u{1f914} Thinking\u{2026}").await?;
    let ack_mid = ack.get("message_id").and_then(|v| v.as_i64()).unwrap_or(0);

    let key = format!("ask:{chat_id}");

    // Session is already primed with item context, so send the raw question.
    let response = match bot
        .gw()
        .post("/api/ops/message", &json!({"key": key, "message": text}))
        .await
    {
        Ok(resp) => super::extract_result_text(&resp),
        Err(e) => format!("\u{274c} Session lost — use /ask to pick the item again: {e}"),
    };

    let display = render_markdown_reply_html(&response, 4000);

    bot.edit_message_with_markup(chat_id, ack_mid, &display, Some(ask_kb()))
        .await?;

    Ok(())
}

/// Start the Claude session with item context so follow-up questions have it.
pub async fn prime_session(bot: &mut TelegramBot, chat_id: &str, item_context: &str) -> Result<()> {
    let key = format!("ask:{chat_id}");
    let prompt = format!(
        "You are answering questions about a completed software task. \
         Be concise and factual.\n\n\
         {item_context}\n\n\
         Look up this item's full details using `mando todo show` with the item ID, \
         then read the relevant PR, branch, and commit history to build context. \
         After gathering context, say \"Ready\" and wait for the user's question.",
    );
    bot.gw()
        .post("/api/ops/start", &json!({"key": key, "prompt": prompt}))
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
