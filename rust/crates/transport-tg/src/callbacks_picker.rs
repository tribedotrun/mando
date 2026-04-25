//! `/action` callback handlers — picker and action execution.

use anyhow::Result;
use serde_json::json;

use crate::bot::TelegramBot;
use crate::commands::action;
use crate::gateway_paths as paths;

/// Handle all `act:*` callbacks.
pub(crate) async fn handle_action_callback(
    bot: &mut TelegramBot,
    parts: &[&str],
    cb_id: &str,
    cid: &str,
    mid: i64,
) -> Result<()> {
    let action = parts.get(1).copied().unwrap_or("");

    match action {
        "cancel" => {
            let aid = parts.get(2).copied().unwrap_or("");
            bot.take_action_picker(aid);
            bot.api()
                .answer_callback_query(cb_id, Some("Cancelled"))
                .await?;
            global_infra::best_effort!(
                bot.edit_message(cid, mid, "\u{23ed} Cancelled").await,
                "callbacks_picker: bot.edit_message(cid, mid, '/u{23ed} Cancelled').await"
            );
        }
        "pick" => handle_pick(bot, parts, cb_id, cid, mid).await?,
        "do" => handle_do(bot, parts, cb_id, cid, mid).await?,
        "ask_end" => {
            if let Some(task_id) = bot.ask_session_task_id(cid) {
                if let Err(e) = bot
                    .gw()
                    .post_typed::<_, api_types::AskEndResponse>(
                        paths::TASKS_ASK_END,
                        &json!({"id": task_id}),
                    )
                    .await
                {
                    tracing::debug!(
                        target: "transport-tg",
                        %e,
                        "best-effort: ask/end post failed",
                    );
                }
            }
            bot.close_ask_session(cid);
            bot.api()
                .answer_callback_query(cb_id, Some("Session ended"))
                .await?;
            global_infra::best_effort!(
                bot.remove_keyboard(cid, mid).await,
                "callbacks_picker: bot.remove_keyboard(cid, mid).await"
            );
            global_infra::best_effort!(
                bot.send_html(cid, "Ask session ended.").await,
                "callbacks_picker: bot.send_html(cid, 'Ask session ended.').await"
            );
        }
        "noop" => {
            bot.api()
                .answer_callback_query(cb_id, Some("No actions available"))
                .await?;
        }
        _ => {
            bot.api().answer_callback_query(cb_id, None).await?;
        }
    }

    Ok(())
}

/// User picked a task from the picker → show action buttons.
async fn handle_pick(
    bot: &mut TelegramBot,
    parts: &[&str],
    cb_id: &str,
    cid: &str,
    mid: i64,
) -> Result<()> {
    let aid = parts.get(2).copied().unwrap_or("");
    let idx: usize = parts
        .get(3)
        .and_then(|s| s.parse().ok())
        .unwrap_or(usize::MAX);

    let picker = bot.take_action_picker(aid);

    match picker {
        Some(p) if idx < p.items.len() => {
            let item = &p.items[idx];
            let task_id = &item.id;
            let title = crate::telegram_format::escape_html(&item.title);
            let status: captain::ItemStatus = item
                .status
                .as_deref()
                .and_then(|s| serde_json::from_value(json!(s)).ok())
                .unwrap_or(captain::ItemStatus::New);

            bot.api()
                .answer_callback_query(cb_id, Some("Choose action"))
                .await?;

            let buttons = action::action_buttons(task_id, status, item.has_pr);
            let msg = format!("\u{2699}\u{fe0f} <b>#{task_id}</b> {title}\n\nChoose an action:");
            let _ignored = bot
                .edit_message_with_markup(
                    cid,
                    mid,
                    &msg,
                    Some(api_types::TelegramReplyMarkup::InlineKeyboard { rows: buttons }),
                )
                .await;
        }
        Some(_) => {
            bot.api()
                .answer_callback_query(cb_id, Some("Out of range"))
                .await?;
        }
        None => {
            bot.api()
                .answer_callback_query(cb_id, Some("Picker expired"))
                .await?;
        }
    }
    Ok(())
}

/// User tapped an action button → execute it.
async fn handle_do(
    bot: &mut TelegramBot,
    parts: &[&str],
    cb_id: &str,
    cid: &str,
    mid: i64,
) -> Result<()> {
    let task_id = parts.get(2).copied().unwrap_or("");
    let action_name = parts.get(3).copied().unwrap_or("");

    bot.api()
        .answer_callback_query(cb_id, Some(&format!("{action_name}\u{2026}")))
        .await?;

    match action_name {
        // Immediate actions
        "merge" => {
            global_infra::best_effort!(
                bot.edit_message(cid, mid, "\u{23f3} Merging\u{2026}").await,
                "callbacks_picker: bot.edit_message(cid, mid, '/u{23f3} Merging/u{2026}').await"
            );
            crate::callback_actions::merge(bot, cid, task_id, Some(mid)).await?;
        }
        "accept" => {
            let _ignored = bot
                .edit_message(cid, mid, "\u{23f3} Accepting\u{2026}")
                .await;
            crate::callback_actions::accept(bot, cid, task_id, Some(mid)).await?;
        }
        "handoff" => {
            let _ignored = bot
                .edit_message(cid, mid, "\u{23f3} Handing off\u{2026}")
                .await;
            crate::callback_actions::handoff(bot, cid, task_id, "").await?;
        }
        "stop" => {
            let _ignored = bot
                .edit_message(cid, mid, "\u{23f3} Stopping\u{2026}")
                .await;
            crate::callback_actions::stop(bot, cid, task_id).await?;
        }
        "cancel" => {
            let _ignored = bot
                .edit_message(cid, mid, "\u{23f3} Cancelling\u{2026}")
                .await;
            let id_num: i64 = task_id.parse().unwrap_or(0);
            match bot
                .gw()
                .post_typed::<_, api_types::BoolOkResponse>(
                    crate::gateway_paths::TASKS_BULK,
                    &json!({"ids": [id_num], "updates": {"status": "canceled"}}),
                )
                .await
            {
                Ok(_) => {
                    bot.send_html(
                        cid,
                        &format!(
                            "\u{274c} Cancelled #{}",
                            crate::telegram_format::escape_html(task_id)
                        ),
                    )
                    .await?;
                }
                Err(e) => {
                    bot.send_html(
                        cid,
                        &format!(
                            "\u{274c} Cancel failed: {}",
                            crate::telegram_format::escape_html(&e.to_string())
                        ),
                    )
                    .await?;
                }
            }
        }
        // Text-requiring actions — prompt for input
        "reopen" => {
            let title = fetch_task_title(bot, task_id).await;
            bot.pending_reopen
                .insert(cid.to_string(), (task_id.to_string(), title.clone()));
            let _ignored = bot
                .edit_message(
                    cid,
                    mid,
                    &format!(
                        "\u{1f504} Reopen: {}\n\nType your feedback \u{2014} what changes are needed?",
                        crate::telegram_format::escape_html(&title)
                    ),
                )
                .await;
        }
        "rework" => {
            let title = fetch_task_title(bot, task_id).await;
            bot.pending_rework
                .insert(cid.to_string(), (task_id.to_string(), title.clone()));
            let _ignored = bot
                .edit_message(
                    cid,
                    mid,
                    &format!(
                        "\u{1f501} Rework: {}\n\nType the new instructions.",
                        crate::telegram_format::escape_html(&title)
                    ),
                )
                .await;
        }
        "nudge" => {
            let title = fetch_task_title(bot, task_id).await;
            bot.pending_nudge
                .insert(cid.to_string(), (task_id.to_string(), title.clone()));
            let _ignored = bot
                .edit_message(
                    cid,
                    mid,
                    &format!(
                        "\u{1f4e3} Nudge: {}\n\nType the message for the worker.",
                        crate::telegram_format::escape_html(&title)
                    ),
                )
                .await;
        }
        // Session-based actions
        "input" => {
            let title = fetch_task_title(bot, task_id).await;
            bot.open_input_session(cid, &title);
            let questions = action::fetch_clarifier_questions(bot, task_id).await;
            let msg = if let Some(ref q) = questions {
                format!(
                    "\u{2753} <b>{}</b>\n\n{}\n\nReply with your answers, or /action cancel.",
                    crate::telegram_format::escape_html(&title),
                    crate::telegram_format::escape_html(q),
                )
            } else {
                format!(
                    "\u{1f9ed} Input: {}\n\nType context, or /action cancel.",
                    crate::telegram_format::escape_html(&title),
                )
            };
            global_infra::best_effort!(
                bot.edit_message(cid, mid, &msg).await,
                "callbacks_picker: bot.edit_message(cid, mid, &msg).await"
            );
        }
        "ask" => {
            let id_num: i64 = task_id.parse().unwrap_or(0);
            bot.close_ask_session(cid);
            bot.open_ask_session(cid, id_num);
            let title = fetch_task_title(bot, task_id).await;
            let _ignored = bot
                .edit_message(
                    cid,
                    mid,
                    &format!(
                        "\u{1f4ac} Ask: {}\n\nType your question.",
                        crate::telegram_format::escape_html(&title)
                    ),
                )
                .await;
        }
        _ => {}
    }

    Ok(())
}

/// Fetch task title by ID from the task list.
async fn fetch_task_title(bot: &TelegramBot, task_id: &str) -> String {
    let id_num: i64 = task_id.parse().unwrap_or(0);
    bot.gw()
        .get_typed::<api_types::TaskListResponse>(crate::gateway_paths::TASKS)
        .await
        .ok()
        .and_then(|r| {
            r.items.into_iter().find_map(|item| {
                if item.id == id_num {
                    Some(item.title)
                } else {
                    None
                }
            })
        })
        .unwrap_or_else(|| format!("#{task_id}"))
}
