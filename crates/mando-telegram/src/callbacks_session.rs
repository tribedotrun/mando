//! Callback handlers for session-based interactions (ops, ask, knowledge).

use anyhow::Result;
use serde_json::json;

use crate::bot::TelegramBot;

pub(crate) async fn handle_ops_callback(
    bot: &mut TelegramBot,
    parts: &[&str],
    cb_id: &str,
    cid: &str,
    mid: i64,
) -> Result<()> {
    let action = parts.get(1).copied().unwrap_or("");
    match action {
        "new" => {
            bot.api().answer_callback_query(cb_id, None).await?;
            // End any existing server-side session before starting fresh.
            let key = format!("ops:{cid}");
            if let Err(e) = bot.gw().post("/api/ops/end", &json!({"key": key})).await {
                tracing::warn!(module = "telegram", error = %e, "failed to end server-side ops session");
            }
            bot.close_ops_session(cid);
            bot.open_ops_session(cid);
            // Preserve previous message content — just strip the keyboard.
            if let Err(e) = bot.remove_keyboard(cid, mid).await {
                tracing::warn!(module = "telegram", error = %e, "message send failed");
            }
            if let Err(e) = bot
                .send_html(cid, "\u{1f527} New ops session. What do you need?")
                .await
            {
                tracing::warn!(module = "telegram", error = %e, "message send failed");
            }
        }
        "end" => {
            bot.api().answer_callback_query(cb_id, None).await?;
            let key = format!("ops:{cid}");
            if let Err(e) = bot.gw().post("/api/ops/end", &json!({"key": key})).await {
                tracing::warn!(module = "telegram", error = %e, "failed to end server-side ops session");
            }
            bot.close_ops_session(cid);
            // Preserve previous message content — just strip the keyboard.
            if let Err(e) = bot.remove_keyboard(cid, mid).await {
                tracing::warn!(module = "telegram", error = %e, "message send failed");
            }
            if let Err(e) = bot.send_html(cid, "\u{1f527} Ops session ended.").await {
                tracing::warn!(module = "telegram", error = %e, "message send failed");
            }
        }
        _ => {
            bot.api().answer_callback_query(cb_id, None).await?;
        }
    }
    Ok(())
}

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
            let key = format!("ask:{cid}");
            if let Err(e) = bot.gw().post("/api/ops/end", &json!({"key": key})).await {
                tracing::warn!(module = "telegram", error = %e, "failed to end server-side ops session");
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
                    bot.close_ask_session(cid);
                    bot.open_ask_session(cid);
                    bot.api()
                        .answer_callback_query(cb_id, Some("Starting\u{2026}"))
                        .await?;
                    if let Err(e) = bot
                        .edit_message(cid, mid, &format!("Ask: {title}\n\nType your question."))
                        .await
                    {
                        tracing::warn!(module = "telegram", error = %e, "message send failed");
                    }

                    // Prime the Claude session with item context so follow-up
                    // questions know which item the user is asking about.
                    let context = format!("Item ID: {}\nTitle: {}", item.id, item.title,);
                    if let Err(e) = crate::commands::ask::prime_session(bot, cid, &context).await {
                        bot.close_ask_session(cid);
                        if let Err(e) = bot
                            .edit_message(
                                cid,
                                mid,
                                &format!(
                                    "Ask: {title}\n\n\u{274c} Failed to start session: {}",
                                    mando_shared::escape_html(&e.to_string()),
                                ),
                            )
                            .await
                        {
                            tracing::warn!(module = "telegram", error = %e, "message send failed");
                        }
                        return Ok(());
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

pub(crate) async fn handle_knowledge_callback(
    bot: &TelegramBot,
    parts: &[&str],
    cb_id: &str,
    cid: &str,
    mid: i64,
) -> Result<()> {
    let action = parts.get(1).copied().unwrap_or("");
    let lesson_id = parts.get(2).copied().unwrap_or("");

    match action {
        "approve" => {
            bot.api()
                .answer_callback_query(cb_id, Some("Approving\u{2026}"))
                .await?;
            crate::callback_actions::approve_knowledge(bot, cid, lesson_id).await?;
            if let Err(e) = bot
                .edit_message(cid, mid, "\u{2705} Lesson approved.")
                .await
            {
                tracing::warn!(module = "telegram", error = %e, "message send failed");
            }
        }
        "reject" => {
            bot.api()
                .answer_callback_query(cb_id, Some("Rejected"))
                .await?;
            if let Err(e) = bot
                .edit_message(cid, mid, "\u{274c} Lesson rejected.")
                .await
            {
                tracing::warn!(module = "telegram", error = %e, "message send failed");
            }
        }
        _ => {
            bot.api().answer_callback_query(cb_id, None).await?;
        }
    }
    Ok(())
}
