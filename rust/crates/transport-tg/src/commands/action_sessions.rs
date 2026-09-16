//! Session text handlers for `/action` input clarification.
//!
//! Extracted from action.rs for file length.

use crate::telegram_format::escape_html;
use anyhow::Result;
use api_types::ItemStatus;
use tracing::{info, warn};

use crate::bot::TelegramBot;

use super::action::status_short;

const RECLARIFYING_FOLLOW_UP_NOTE: &str =
    "We'll send the next question here when ready. Tap Answer or use /action to reply.";

// ── Input text handler (multi-turn clarification) ───────────────────

/// Handle plain-text messages for active input session. Returns `true` if consumed.
pub async fn handle_input_text(bot: &TelegramBot, chat_id: &str, text: &str) -> Result<bool> {
    let session = match bot.input_session(chat_id).await {
        Some(session) => session,
        None => return Ok(false),
    };
    let task_id = session.task_id;
    let item_title = session.title;
    let _context_append_guard = bot.lock_context_append(task_id).await;

    let tasks_resp = bot
        .gw()
        .get_tasks(&api_types::TaskListQuery {
            include_archived: None,
        })
        .await?;

    let item = match tasks_resp
        .items
        .into_iter()
        .find(|candidate| candidate.id == task_id)
    {
        Some(item) => item,
        None => {
            bot.close_input_session(chat_id).await;
            bot.send_html(chat_id, "\u{26a0}\u{fe0f} Task no longer exists.")
                .await?;
            return Ok(true);
        }
    };

    let existing_context = item.context.clone();
    match item.status {
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
            .post_tasks_by_id_clarify(
                &api_types::TaskIdParams { id: item_id },
                &api_types::ClarifyQuery { wait: Some(false) },
                &api_types::ClarifyRequest {
                    answers: None,
                    answer: Some(text.to_string()),
                },
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
                append_context_fallback(
                    bot,
                    chat_id,
                    ack_mid,
                    &item_title,
                    item_id,
                    existing_context.as_deref(),
                    text,
                )
                .await;
            }
        }
    } else {
        append_context_fallback(
            bot,
            chat_id,
            ack_mid,
            &item_title,
            item_id,
            existing_context.as_deref(),
            text,
        )
        .await;
    }

    Ok(true)
}

async fn append_context_fallback(
    bot: &TelegramBot,
    chat_id: &str,
    mid: i64,
    title: &str,
    item_id: i64,
    existing_context: Option<&str>,
    text: &str,
) {
    let existing = existing_context.unwrap_or_default().trim();
    let appended = if existing.is_empty() {
        format!("Human note: {text}")
    } else {
        format!("{existing}\n\nHuman note: {text}")
    };

    match bot
        .gw()
        .patch_tasks_by_id(
            &api_types::TaskIdParams { id: item_id },
            &api_types::TaskPatchRequest {
                context: Some(appended),
                original_prompt: None,
                is_bug_fix: None,
            },
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

// ── Clarifier question fetch (for input sessions) ───────────────────

/// Fetch the latest clarifier questions for a task from the timeline and
/// format them for display in Telegram. Filters out self-answered entries
/// to mirror the Electron renderer's behavior.
pub(crate) async fn fetch_clarifier_questions(bot: &TelegramBot, item_id: &str) -> Option<String> {
    let id = item_id.parse().ok()?;
    let timeline = bot
        .gw()
        .get_tasks_by_id_timeline(&api_types::TaskIdParams { id })
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
