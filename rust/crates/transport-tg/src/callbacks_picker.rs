//! `/action` callback handlers — picker and action execution.

use anyhow::Result;

use crate::bot::TelegramBot;
use crate::commands::action;

/// Handle all `act:*` callbacks.
pub(crate) async fn handle_action_callback(
    bot: &TelegramBot,
    parts: &[&str],
    cb_id: &str,
    cid: &str,
    mid: i64,
) -> Result<()> {
    let action = parts.get(1).copied().unwrap_or("");

    match action {
        "cancel" => {
            let aid = parts.get(2).copied().unwrap_or("");
            bot.take_action_picker(aid).await;
            bot.api()
                .answer_callback_query(cb_id, Some("Cancelled"))
                .await?;
            global_infra::best_effort!(
                bot.edit_message(cid, mid, "\u{23ed} Cancelled").await,
                "callbacks_picker: edit on action-picker cancel"
            );
        }
        "pick" => handle_pick(bot, parts, cb_id, cid, mid).await?,
        "do" => handle_do(bot, parts, cb_id, cid, mid).await?,
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
    bot: &TelegramBot,
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

    let picker = bot.take_action_picker(aid).await;

    match picker {
        Some(p) if idx < p.items.len() => {
            let item = &p.items[idx];
            let task_id = &item.id;
            let title = crate::telegram_format::escape_html(&item.title);
            let status = item.status.unwrap_or(api_types::ItemStatus::New);

            bot.api()
                .answer_callback_query(cb_id, Some("Choose action"))
                .await?;

            let buttons = action::action_buttons(task_id, status, item.has_pr);
            let msg = format!("\u{2699}\u{fe0f} <b>#{task_id}</b> {title}\n\nChoose an action:");
            global_infra::best_effort!(
                bot.edit_message_with_markup(
                    cid,
                    mid,
                    &msg,
                    Some(api_types::TelegramReplyMarkup::InlineKeyboard { rows: buttons }),
                )
                .await,
                "callbacks_picker: show task actions"
            );
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
    bot: &TelegramBot,
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
                "callbacks_picker: edit on merge action"
            );
            crate::callback_actions::merge(bot, cid, task_id, Some(mid)).await?;
        }
        "accept" => {
            global_infra::best_effort!(
                bot.edit_message(cid, mid, "\u{23f3} Accepting\u{2026}")
                    .await,
                "callbacks_picker: show accept progress"
            );
            crate::callback_actions::accept(bot, cid, task_id, Some(mid)).await?;
        }
        "handoff" => {
            global_infra::best_effort!(
                bot.edit_message(cid, mid, "\u{23f3} Handing off\u{2026}")
                    .await,
                "callbacks_picker: show handoff progress"
            );
            crate::callback_actions::handoff(bot, cid, task_id, "").await?;
        }
        "stop" => {
            global_infra::best_effort!(
                bot.edit_message(cid, mid, "\u{23f3} Stopping\u{2026}")
                    .await,
                "callbacks_picker: show stop progress"
            );
            crate::callback_actions::stop(bot, cid, task_id).await?;
        }
        "cancel" => {
            global_infra::best_effort!(
                bot.edit_message(cid, mid, "\u{23f3} Cancelling\u{2026}")
                    .await,
                "callbacks_picker: show cancel progress"
            );
            let id_num: i64 = task_id.parse().unwrap_or(0);
            match bot
                .gw()
                .post_tasks_cancel(&api_types::TaskIdRequest { id: id_num })
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
        // Text-requiring actions — edit the picker message into a prompt and
        // register the pending session against that message id, so a
        // quote-reply routes back here under concurrent dispatch.
        "reopen" => {
            let title = fetch_task_title(bot, task_id).await;
            global_infra::best_effort!(
                bot.edit_message(
                    cid,
                    mid,
                    &format!(
                        "\u{1f504} Reopen: {}\n\nReply to this prompt with your feedback \u{2014} what changes are needed?",
                        crate::telegram_format::escape_html(&title)
                    ),
                )
                .await,
                "callbacks_picker: show reopen prompt"
            );
            bot.set_pending_reopen(cid, task_id, &title, mid).await;
        }
        "rework" => {
            let title = fetch_task_title(bot, task_id).await;
            global_infra::best_effort!(
                bot.edit_message(
                    cid,
                    mid,
                    &format!(
                        "\u{1f501} Rework: {}\n\nReply to this prompt with the new instructions.",
                        crate::telegram_format::escape_html(&title)
                    ),
                )
                .await,
                "callbacks_picker: show rework prompt"
            );
            bot.set_pending_rework(cid, task_id, &title, mid).await;
        }
        "nudge" => {
            let title = fetch_task_title(bot, task_id).await;
            global_infra::best_effort!(
                bot.edit_message(
                    cid,
                    mid,
                    &format!(
                        "\u{1f4e3} Nudge: {}\n\nReply to this prompt with the message for the worker.",
                        crate::telegram_format::escape_html(&title)
                    ),
                )
                .await,
                "callbacks_picker: show nudge prompt"
            );
            bot.set_pending_nudge(cid, task_id, &title, mid).await;
        }
        // Session-based actions
        "input" => {
            let id_num =
                global_types::parse_i64_id(task_id, "task").map_err(|e| anyhow::anyhow!(e))?;
            let title = fetch_task_title(bot, task_id).await;
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
                "callbacks_picker: edit on input prompt"
            );
            bot.open_input_session(cid, id_num, &title, mid).await;
        }
        _ => {}
    }

    Ok(())
}

/// Fetch task title by ID from the task list.
async fn fetch_task_title(bot: &TelegramBot, task_id: &str) -> String {
    let id_num: i64 = task_id.parse().unwrap_or(0);
    bot.gw()
        .get_tasks(&api_types::TaskListQuery {
            include_archived: None,
        })
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
