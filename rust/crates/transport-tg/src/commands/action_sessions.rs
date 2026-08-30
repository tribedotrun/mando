//! Session text handlers for `/action` — input clarification and ask Q&A.
//!
//! Extracted from action.rs for file length.

use crate::telegram_format::escape_html;
use anyhow::Result;
use captain::ItemStatus;
use serde_json::json;
use tracing::{info, warn};

use crate::bot::TelegramBot;
use crate::gateway_paths as paths;

use super::action::status_short;

const RECLARIFYING_FOLLOW_UP_NOTE: &str =
    "We'll send the next question here when ready. Tap Answer or use /action to reply.";

// ── Input text handler (multi-turn clarification) ───────────────────

/// Handle plain-text messages for active input session. Returns `true` if consumed.
pub async fn handle_input_text(bot: &TelegramBot, chat_id: &str, text: &str) -> Result<bool> {
    let item_title = match bot.input_session_title(chat_id).await {
        Some(t) => t,
        None => return Ok(false),
    };

    let tasks_resp = bot
        .gw()
        .get_typed::<api_types::TaskListResponse>(paths::TASKS)
        .await?;

    // Fail-fast on serde drift: propagate the error instead of dropping
    // the matching task into the "not found" bucket. A schema mismatch
    // is an infrastructure error and must surface, not be papered over
    // with "Task no longer exists".
    let mut item: Option<captain::Task> = None;
    for candidate in tasks_resp.items {
        if candidate.title != item_title {
            continue;
        }
        let task_id = candidate.id;
        let value = serde_json::to_value(&candidate).map_err(|e| {
            anyhow::anyhow!("failed to serialize TaskItem {task_id} during sessions lookup: {e}")
        })?;
        let task: captain::Task = serde_json::from_value(value).map_err(|e| {
            anyhow::anyhow!(
                "failed to convert TaskItem {task_id} to Task during sessions lookup (api-types schema drift): {e}"
            )
        })?;
        item = Some(task);
        break;
    }

    let item = match item {
        Some(it) => it,
        None => {
            bot.close_input_session(chat_id).await;
            bot.send_html(chat_id, "\u{26a0}\u{fe0f} Task no longer exists.")
                .await?;
            return Ok(true);
        }
    };

    match item.status() {
        ItemStatus::New
        | ItemStatus::Clarifying
        | ItemStatus::NeedsClarification
        | ItemStatus::Queued => {}
        _ => {
            bot.close_input_session(chat_id).await;
            bot.send_html(
                chat_id,
                &format!(
                    "\u{2139}\u{fe0f} Task is now {}. Use /action to pick again.",
                    status_short(item.status())
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

    if item.status() == ItemStatus::NeedsClarification {
        match bot
            .gw()
            .post_typed::<_, api_types::ClarifyResponse>(
                &paths::task_clarify_async(item_id),
                &json!({"answer": text}),
            )
            .await
        {
            Ok(_) => {
                // Async ack: the daemon committed the answer and spawned the
                // follow-up CC call on its task tracker. The next question (or
                // ready/escalate state) arrives via the existing TG notify
                // path, so close this session. The user re-engages via the
                // notification's Answer button or `/action` when the next
                // message lands.
                bot.close_input_session(chat_id).await;
                global_infra::best_effort!(
                    bot.edit_message(
                        chat_id,
                        ack_mid,
                        &format!(
                            "\u{1f9ed} Got your answer for <b>{}</b>.\n\n{}",
                            escape_html(&item_title),
                            RECLARIFYING_FOLLOW_UP_NOTE,
                        ),
                    )
                    .await,
                    "action_sessions: ack edit after async clarify"
                );
            }
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

async fn append_context_fallback(
    bot: &TelegramBot,
    chat_id: &str,
    mid: i64,
    title: &str,
    item_id: i64,
    text: &str,
) {
    let existing = bot
        .gw()
        .get_typed::<api_types::TaskListResponse>(paths::TASKS)
        .await
        .ok()
        .and_then(|r| {
            r.items.into_iter().find_map(|item| {
                if item.id == item_id {
                    item.context
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
        .patch_typed::<_, api_types::BoolOkResponse>(
            &paths::task_item(item_id),
            &json!({"context": appended}),
        )
        .await
    {
        Ok(_) => {
            info!("Input: appended context for '{}'", title);
            global_infra::best_effort!(
                bot.edit_message(
                    chat_id,
                    mid,
                    &format!(
                        "\u{2705} Context appended to <b>{}</b>.",
                        escape_html(title)
                    ),
                )
                .await,
                "action_sessions: bot .edit_message( chat_id, mid, &format!( '\u{2705} Context"
            );
        }
        Err(e) => {
            warn!("Input: context append failed for '{}': {e}", title);
            global_infra::best_effort!(
                bot.edit_message(chat_id, mid, "\u{274c} Failed to append context.")
                    .await,
                "action_sessions: bot .edit_message(chat_id, mid, '\u{274c} Failed to append c"
            );
        }
    }
    bot.close_input_session(chat_id).await;
}

// ── Ask text handler (multi-turn Q&A) ───────────────────────────────

/// Handle plain-text messages for active ask session. Returns `true` if consumed.
pub async fn handle_ask_text(bot: &TelegramBot, chat_id: &str, text: &str) -> Result<bool> {
    if !bot.has_ask_session(chat_id).await {
        return Ok(false);
    }
    ask_turn(bot, chat_id, text).await?;
    Ok(true)
}

/// Execute one ask turn.
pub(crate) async fn ask_turn(bot: &TelegramBot, chat_id: &str, text: &str) -> Result<()> {
    let task_id = match bot.ask_session_task_id(chat_id).await {
        Some(id) => id,
        None => {
            bot.close_ask_session(chat_id).await;
            bot.send_html(
                chat_id,
                "Ask session lost \u{2014} use /action to pick a task.",
            )
            .await?;
            return Ok(());
        }
    };

    bot.increment_ask_rounds(chat_id).await;

    let ack = bot.send_html(chat_id, "\u{1f914} Thinking\u{2026}").await?;
    let ack_mid = ack.get("message_id").and_then(|v| v.as_i64()).unwrap_or(0);

    let response = match bot
        .gw()
        .post_typed::<_, api_types::AskResponse>(
            paths::TASKS_ASK,
            &json!({"id": task_id, "question": text}),
        )
        .await
    {
        Ok(resp) => resp.answer,
        Err(e) => format!("\u{274c} Ask failed: {}", escape_html(&e.to_string())),
    };

    let display = crate::telegram_format::render_markdown_reply_html(&response, 4000);
    let kb = api_types::TelegramReplyMarkup::InlineKeyboard {
        rows: vec![vec![api_types::InlineKeyboardButton {
            text: "End session".into(),
            callback_data: Some("act:ask_end".into()),
            url: None,
        }]],
    };

    bot.edit_message_with_markup(chat_id, ack_mid, &display, Some(kb))
        .await?;

    Ok(())
}

// ── Clarifier question fetch (for input sessions) ───────────────────

/// Fetch the latest clarifier questions for a task from the timeline and
/// format them for display in Telegram. Filters out self-answered entries
/// to mirror the Electron renderer's behavior.
pub(crate) async fn fetch_clarifier_questions(bot: &TelegramBot, item_id: &str) -> Option<String> {
    let path = paths::task_timeline(item_id);
    let timeline = bot
        .gw()
        .get_typed::<api_types::TimelineResponse>(&path)
        .await
        .ok()?;
    let questions = timeline.events.iter().rev().find_map(|e| match &e.data {
        api_types::TimelineEventPayload::ClarifyQuestion { questions: qs, .. }
            if !qs.is_empty() =>
        {
            Some(qs)
        }
        _ => None,
    })?;
    let lines: Vec<String> = questions
        .iter()
        .filter(|q| !q.self_answered)
        .enumerate()
        .map(|(i, q)| format!("{}. {}", i + 1, q.question))
        .collect();
    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n"))
    }
}
