//! Session text handlers for `/action` — input clarification and ask Q&A.
//!
//! Extracted from action.rs for file length.

use anyhow::Result;
use mando_shared::telegram_format::escape_html;
use mando_types::task::ItemStatus;
use serde_json::json;
use tracing::{info, warn};

use crate::bot::TelegramBot;

use super::action::status_short;

// ── Input text handler (multi-turn clarification) ───────────────────

/// Handle plain-text messages for active input session. Returns `true` if consumed.
pub async fn handle_input_text(bot: &mut TelegramBot, chat_id: &str, text: &str) -> Result<bool> {
    let item_title = match bot.input_session_title(chat_id) {
        Some(t) => t,
        None => return Ok(false),
    };

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
            bot.close_input_session(chat_id);
            bot.send_html(chat_id, "\u{26a0}\u{fe0f} Task no longer exists.")
                .await?;
            return Ok(true);
        }
    };

    match item.status {
        ItemStatus::New
        | ItemStatus::Clarifying
        | ItemStatus::NeedsClarification
        | ItemStatus::Queued => {}
        _ => {
            bot.close_input_session(chat_id);
            bot.send_html(
                chat_id,
                &format!(
                    "\u{2139}\u{fe0f} Task is now {}. Use /action to pick again.",
                    status_short(item.status)
                ),
            )
            .await?;
            return Ok(true);
        }
    }

    let item_id = item.id;
    let ack = bot
        .send_html(chat_id, "\u{1f9ed} Clarifying\u{2026}")
        .await?;
    let ack_mid = ack.get("message_id").and_then(|v| v.as_i64()).unwrap_or(0);

    if item.status == ItemStatus::NeedsClarification {
        match bot
            .gw()
            .post(
                &format!("/api/tasks/{item_id}/clarify"),
                &json!({"answer": text}),
            )
            .await
        {
            Ok(resp) => handle_clarify_response(bot, chat_id, ack_mid, &item_title, &resp).await,
            Err(e) => {
                info!("[input] clarify failed for '{}': {}", item_title, e);
                append_context_fallback(bot, chat_id, ack_mid, &item_title, item_id, text).await;
            }
        }
    } else {
        append_context_fallback(bot, chat_id, ack_mid, &item_title, item_id, text).await;
    }

    Ok(true)
}

async fn handle_clarify_response(
    bot: &mut TelegramBot,
    chat_id: &str,
    mid: i64,
    title: &str,
    resp: &serde_json::Value,
) {
    let status = resp.get("status").and_then(|v| v.as_str()).unwrap_or("");
    match status {
        "ready" => {
            bot.close_input_session(chat_id);
            let _ = bot
                .edit_message(
                    chat_id,
                    mid,
                    &format!("\u{2705} Clarified <b>{}</b>.", escape_html(title)),
                )
                .await;
        }
        "clarifying" | "escalate" => {
            let q = resp
                .get("questions")
                .and_then(|v| v.as_str())
                .unwrap_or("More details?");
            let _ = bot
                .edit_message(
                    chat_id,
                    mid,
                    &format!(
                        "\u{1f9ed} {}\n\nReply here, or /action cancel.",
                        escape_html(q)
                    ),
                )
                .await;
        }
        "needs-clarification" => {
            let error = resp
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("LLM unavailable");
            let q = resp.get("questions").and_then(|v| v.as_str()).unwrap_or("");
            let msg = if q.is_empty() {
                format!(
                    "\u{26a0}\u{fe0f} Answer saved, re-clarification failed ({}).",
                    escape_html(error)
                )
            } else {
                format!(
                    "\u{26a0}\u{fe0f} Answer saved, re-clarification failed.\n\n{}\n\nReply or /action cancel.",
                    escape_html(q)
                )
            };
            let _ = bot.edit_message(chat_id, mid, &msg).await;
        }
        _ => {
            let _ = bot
                .edit_message(
                    chat_id,
                    mid,
                    &format!("\u{2705} Updated <b>{}</b>.", escape_html(title)),
                )
                .await;
        }
    }
}

async fn append_context_fallback(
    bot: &mut TelegramBot,
    chat_id: &str,
    mid: i64,
    title: &str,
    item_id: i64,
    text: &str,
) {
    let existing = bot
        .gw()
        .get("/api/tasks")
        .await
        .ok()
        .and_then(|r| {
            r.get("items")?.as_array()?.iter().find_map(|v| {
                if v.get("id")?.as_i64()? == item_id {
                    v.get("context").and_then(|c| c.as_str()).map(String::from)
                } else {
                    None
                }
            })
        })
        .unwrap_or_default();

    let appended = if existing.trim().is_empty() {
        format!("Human note: {text}")
    } else {
        format!("{}\n\nHuman note: {text}", existing.trim())
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
            info!("Input: appended context for '{}'", title);
            let _ = bot
                .edit_message(
                    chat_id,
                    mid,
                    &format!(
                        "\u{2705} Context appended to <b>{}</b>.",
                        escape_html(title)
                    ),
                )
                .await;
        }
        Err(e) => {
            warn!("Input: context append failed for '{}': {e}", title);
            let _ = bot
                .edit_message(chat_id, mid, "\u{274c} Failed to append context.")
                .await;
        }
    }
    bot.close_input_session(chat_id);
}

// ── Ask text handler (multi-turn Q&A) ───────────────────────────────

/// Handle plain-text messages for active ask session. Returns `true` if consumed.
pub async fn handle_ask_text(bot: &mut TelegramBot, chat_id: &str, text: &str) -> Result<bool> {
    if !bot.has_ask_session(chat_id) {
        return Ok(false);
    }
    ask_turn(bot, chat_id, text).await?;
    Ok(true)
}

/// Execute one ask turn.
pub(crate) async fn ask_turn(bot: &mut TelegramBot, chat_id: &str, text: &str) -> Result<()> {
    let task_id = match bot.ask_session_task_id(chat_id) {
        Some(id) => id,
        None => {
            bot.close_ask_session(chat_id);
            bot.send_html(
                chat_id,
                "Ask session lost \u{2014} use /action to pick a task.",
            )
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

    let display = mando_shared::render_markdown_reply_html(&response, 4000);
    let kb = json!({
        "inline_keyboard": [[
            {"text": "End session", "callback_data": "act:ask_end"},
        ]]
    });

    bot.edit_message_with_markup(chat_id, ack_mid, &display, Some(kb))
        .await?;

    Ok(())
}

// ── Clarifier question fetch (for input sessions) ───────────────────

/// Fetch the latest clarifier questions for a task from the timeline.
pub(crate) async fn fetch_clarifier_questions(bot: &TelegramBot, item_id: &str) -> Option<String> {
    let path = format!("/api/tasks/{item_id}/timeline");
    let val = bot.gw().get(&path).await.ok()?;
    let events = val["events"].as_array()?;
    events
        .iter()
        .rev()
        .find(|e| e["event_type"].as_str() == Some("clarify_question"))
        .and_then(|e| {
            let q = &e["data"]["questions"];
            if let Some(arr) = q.as_array() {
                let lines: Vec<String> = arr
                    .iter()
                    .filter(|item| !item["self_answered"].as_bool().unwrap_or(false))
                    .enumerate()
                    .map(|(i, item)| {
                        format!("{}. {}", i + 1, item["question"].as_str().unwrap_or("?"))
                    })
                    .collect();
                if lines.is_empty() {
                    None
                } else {
                    Some(lines.join("\n"))
                }
            } else {
                q.as_str().map(String::from)
            }
        })
}
