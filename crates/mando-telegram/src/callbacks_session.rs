//! Callback handlers for session-based interactions (ask).

use anyhow::Result;
use serde_json::json;

use crate::bot::TelegramBot;

pub(crate) async fn handle_ask_callback(
    bot: &mut TelegramBot,
    parts: &[&str],
    cb_id: &str,
    cid: &str,
    mid: i64,
) -> Result<()> {
    let action = parts.get(1).copied().unwrap_or("");
    match action {
        "end" => {
            bot.api()
                .answer_callback_query(cb_id, Some("Session ended"))
                .await?;
            if let Some(task_id) = bot.ask_session_task_id(cid) {
                if let Err(e) = bot
                    .gw()
                    .post("/api/tasks/ask/end", &json!({"id": task_id}))
                    .await
                {
                    tracing::warn!(module = "telegram", error = %e, "failed to end ask session");
                }
            }
            bot.close_ask_session(cid);
            // Preserve previous message content — just strip the keyboard.
            if let Err(e) = bot.remove_keyboard(cid, mid).await {
                tracing::warn!(module = "telegram", error = %e, "message send failed");
            }
            if let Err(e) = bot.send_html(cid, "Ask session ended.").await {
                tracing::warn!(module = "telegram", error = %e, "message send failed");
            }
        }
        "cancel" => {
            let aid = parts.get(2).copied().unwrap_or("");
            bot.take_ask_picker(aid);
            bot.api()
                .answer_callback_query(cb_id, Some("Cancelled"))
                .await?;
            if let Err(e) = bot.edit_message(cid, mid, "Cancelled").await {
                tracing::warn!(module = "telegram", error = %e, "message send failed");
            }
        }
        "pick" => {
            let aid = parts.get(2).copied().unwrap_or("");
            let idx: usize = parts
                .get(3)
                .and_then(|s| s.parse().ok())
                .unwrap_or(usize::MAX);
            if let Some(picker) = bot.take_ask_picker(aid) {
                if idx < picker.items.len() {
                    let item = &picker.items[idx];
                    let title = mando_shared::escape_html(&item.title);
                    match item.id.parse::<i64>() {
                        Ok(task_id) => {
                            bot.close_ask_session(cid);
                            bot.open_ask_session(cid, task_id);
                            bot.api()
                                .answer_callback_query(cb_id, Some("Ready"))
                                .await?;
                            if let Err(e) = bot
                                .edit_message(
                                    cid,
                                    mid,
                                    &format!("Ask: {title}\n\nType your question."),
                                )
                                .await
                            {
                                tracing::warn!(module = "telegram", error = %e, "message send failed");
                            }
                        }
                        Err(e) => {
                            tracing::warn!(module = "telegram", raw_id = %item.id, error = %e, "invalid task ID in picker");
                            bot.api()
                                .answer_callback_query(cb_id, Some("Invalid task ID"))
                                .await?;
                        }
                    }
                } else {
                    bot.api()
                        .answer_callback_query(cb_id, Some("Out of range"))
                        .await?;
                }
            } else {
                bot.api()
                    .answer_callback_query(cb_id, Some("Picker expired"))
                    .await?;
            }
        }
        _ => {
            bot.api().answer_callback_query(cb_id, None).await?;
        }
    }
    Ok(())
}
