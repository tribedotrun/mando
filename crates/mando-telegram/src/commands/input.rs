//! `/input [cancel]` — interactive multi-turn clarifier sessions via HTTP.

use crate::bot::TelegramBot;
use anyhow::Result;
use mando_shared::telegram_format::escape_html;
use mando_types::ItemStatus;
use serde_json::json;
use tracing::{info, warn};

/// Handle `/input [cancel]`.
pub async fn handle(bot: &mut TelegramBot, chat_id: &str, args: &str) -> Result<()> {
    let subcmd = args.trim().to_lowercase();

    if subcmd == "cancel" {
        if bot.has_input_session(chat_id) {
            // Cancel server-side clarifier session.
            bot.gw()
                .post("/api/clarifier/cancel", &json!({"key": chat_id}))
                .await
                .ok();
            bot.close_input_session(chat_id);
            bot.send_html(chat_id, "\u{2705} Clarifier session cancelled.")
                .await?;
        } else {
            bot.send_html(
                chat_id,
                "No active clarifier session. Send /input to start.",
            )
            .await?;
        }
        return Ok(());
    }

    // Check for existing session
    if bot.has_input_session(chat_id) {
        let title = bot.input_session_title(chat_id).unwrap_or_default();
        let msg = format!(
            "\u{1f9ed} Clarifier session already active for:\n\u{2022} {}\n\n\
             Reply with details here, or send /input cancel to exit.",
            escape_html(&title)
        );
        bot.send_html(chat_id, &msg).await?;
        return Ok(());
    }

    // Load task items needing input
    let items = match super::load_tasks_or_notify(bot, chat_id).await {
        Some(items) => items,
        None => return Ok(()),
    };
    let inputable: Vec<_> = items
        .iter()
        .filter(|it| {
            matches!(
                it.status,
                ItemStatus::New
                    | ItemStatus::Clarifying
                    | ItemStatus::NeedsClarification
                    | ItemStatus::Queued
            )
        })
        .collect();

    if inputable.is_empty() {
        bot.send_html(chat_id, "\u{2705} No items awaiting input.")
            .await?;
        return Ok(());
    }

    // Show picker
    let action_id = super::short_uuid();
    let mut lines = vec!["Pick an item to clarify:".to_string()];
    let mut buttons: Vec<serde_json::Value> = Vec::new();

    for (idx, item) in inputable.iter().enumerate() {
        let num = idx + 1;
        let title = escape_html(&item.title);
        let icon = if item.status == ItemStatus::NeedsClarification {
            "\u{2753} "
        } else {
            ""
        };
        lines.push(format!(" {num}. {icon}{title}"));
        buttons.push(json!([{
            "text": format!("Pick #{num}"),
            "callback_data": format!("swarm_input:pick:{action_id}:{idx}"),
        }]));
    }
    buttons.push(json!([{
        "text": "Cancel",
        "callback_data": format!("swarm_input:cancel:{action_id}"),
    }]));

    bot.store_input_picker(&action_id, chat_id, &inputable);

    bot.api()
        .send_message(
            chat_id,
            &lines.join("\n"),
            Some("HTML"),
            Some(json!({"inline_keyboard": buttons})),
            true,
        )
        .await?;

    Ok(())
}

/// Handle plain-text messages routed to an active input session.
/// Routes through HTTP clarifier endpoints. Returns `true` if consumed.
pub async fn handle_text(bot: &mut TelegramBot, chat_id: &str, text: &str) -> Result<bool> {
    let item_title = match bot.input_session_title(chat_id) {
        Some(t) => t,
        None => return Ok(false),
    };

    // Look up item by title via tasks API.
    let tasks_resp = bot.gw().get("/api/tasks").await?;
    let items_val = tasks_resp
        .get("items")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();

    let item: Option<mando_types::Task> = items_val.iter().find_map(|v| {
        let title = v.get("title")?.as_str()?;
        if title == item_title {
            serde_json::from_value(v.clone()).ok()
        } else {
            None
        }
    });

    let item = match item {
        Some(it) => it,
        None => {
            bot.close_input_session_with_cancel(chat_id).await;
            bot.send_html(
                chat_id,
                "\u{26a0}\u{fe0f} Item no longer exists in task list.",
            )
            .await?;
            return Ok(true);
        }
    };

    // Only accept input for pre-work statuses
    match item.status {
        ItemStatus::New
        | ItemStatus::Clarifying
        | ItemStatus::NeedsClarification
        | ItemStatus::Queued => {}
        _ => {
            bot.close_input_session_with_cancel(chat_id).await;
            let status = format!("{:?}", item.status);
            bot.send_html(
                chat_id,
                &format!(
                    "\u{2139}\u{fe0f} Item status changed to {status}. Send /input to start over."
                ),
            )
            .await?;
            return Ok(true);
        }
    }

    let item_id = item.id.to_string();

    let ack = bot
        .send_html(chat_id, "\u{1f9ed} Clarifying\u{2026}")
        .await?;
    let ack_mid = ack.get("message_id").and_then(|v| v.as_i64()).unwrap_or(0);

    // Try follow-up on existing session; if none exists, start fresh.
    let result = match bot
        .gw()
        .post(
            "/api/clarifier/message",
            &json!({"key": chat_id, "message": text}),
        )
        .await
    {
        Ok(resp) => resp,
        Err(_) => {
            // No existing session — start a new one.
            match bot
                .gw()
                .post(
                    "/api/clarifier/start",
                    &json!({"key": chat_id, "item_id": item_id, "message": text}),
                )
                .await
            {
                Ok(resp) => resp,
                Err(e) => {
                    info!("[input] clarifier CC failed for '{}': {}", item_title, e);
                    // Fallback: append context directly via PATCH.
                    append_context_fallback(bot, chat_id, ack_mid, &item_title, &item_id, text)
                        .await;
                    return Ok(true);
                }
            }
        }
    };

    handle_clarifier_response(bot, chat_id, ack_mid, &item_title, &result).await;
    Ok(true)
}

/// Process clarifier response from the gateway.
async fn handle_clarifier_response(
    bot: &mut TelegramBot,
    chat_id: &str,
    message_id: i64,
    item_title: &str,
    resp: &serde_json::Value,
) {
    let result = resp.get("result").unwrap_or(resp);
    let status = result.get("status").and_then(|v| v.as_str()).unwrap_or("");

    match status {
        "Ready" => {
            bot.close_input_session_with_cancel(chat_id).await;
            let _ = bot
                .edit_message(
                    chat_id,
                    message_id,
                    &format!(
                        "\u{2705} Clarified <b>{}</b>. Context enriched and ready.",
                        escape_html(item_title)
                    ),
                )
                .await;
        }
        "Clarifying" | "Escalate" => {
            let questions = result
                .get("questions")
                .and_then(|v| v.as_str())
                .unwrap_or("Can you provide more details?");
            let _ = bot
                .edit_message(
                    chat_id,
                    message_id,
                    &format!(
                        "\u{1f9ed} {}\n\nReply here, or /input cancel to exit.",
                        escape_html(questions)
                    ),
                )
                .await;
        }
        _ => {
            let _ = bot
                .edit_message(
                    chat_id,
                    message_id,
                    &format!("\u{2705} Updated <b>{}</b>.", escape_html(item_title)),
                )
                .await;
        }
    }
}

/// Fallback when CC is unavailable: append human text to context via PATCH.
async fn append_context_fallback(
    bot: &mut TelegramBot,
    chat_id: &str,
    message_id: i64,
    item_title: &str,
    item_id: &str,
    text: &str,
) {
    // Fetch existing context from task list.
    let existing_ctx = bot
        .gw()
        .get("/api/tasks")
        .await
        .ok()
        .and_then(|resp| {
            let items = resp.get("items")?.as_array()?;
            items.iter().find_map(|v| {
                let v_id = v.get("id")?.as_i64()?.to_string();
                if v_id == item_id {
                    v.get("context").and_then(|c| c.as_str()).map(String::from)
                } else {
                    None
                }
            })
        })
        .unwrap_or_default();

    let appended = if existing_ctx.trim().is_empty() {
        format!("Human note: {text}")
    } else {
        format!("{}\n\nHuman note: {text}", existing_ctx.trim())
    };

    match bot
        .gw()
        .patch(
            &format!("/api/tasks/{item_id}"),
            &json!({"context": appended}),
        )
        .await
    {
        Ok(_) => {
            info!(
                "Input session: appended context for '{}' (fallback)",
                item_title
            );
            let _ = bot
                .edit_message(
                    chat_id,
                    message_id,
                    &format!(
                        "\u{2705} Context appended to <b>{}</b> (CC unavailable).",
                        escape_html(item_title)
                    ),
                )
                .await;
        }
        Err(e) => {
            warn!(
                "Input session: failed to append context for '{}': {e}",
                item_title
            );
            let _ = bot
                .edit_message(
                    chat_id,
                    message_id,
                    &format!(
                        "\u{274c} Failed to append context to <b>{}</b>: gateway unreachable.",
                        escape_html(item_title)
                    ),
                )
                .await;
        }
    }
    bot.close_input_session(chat_id);
}
