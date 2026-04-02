//! Generic picker callbacks (input/reopen/rework/handoff).

use anyhow::Result;

use crate::bot::TelegramBot;

pub(crate) async fn handle_picker(
    bot: &mut TelegramBot,
    kind: &str,
    parts: &[&str],
    cb_id: &str,
    cid: &str,
    mid: i64,
) -> Result<()> {
    let action = parts.get(1).copied().unwrap_or("");
    let aid = parts.get(2).copied().unwrap_or("");

    if action == "cancel" {
        take_picker_by_kind(bot, kind, aid);
        bot.api()
            .answer_callback_query(cb_id, Some("Cancelled"))
            .await?;
        if let Err(e) = bot.edit_message(cid, mid, "\u{23ed} Cancelled").await {
            tracing::warn!(module = "telegram", error = %e, "message send failed");
        }
        return Ok(());
    }
    if action != "pick" {
        bot.api().answer_callback_query(cb_id, None).await?;
        return Ok(());
    }
    let idx: usize = parts
        .get(3)
        .and_then(|s| s.parse().ok())
        .unwrap_or(usize::MAX);
    let picker = take_picker_by_kind(bot, kind, aid);

    match picker {
        Some(p) if idx < p.items.len() => {
            let item = &p.items[idx];
            let item_id = &item.id;
            let raw_title = &item.title;
            let title = mando_shared::escape_html(raw_title);
            bot.api()
                .answer_callback_query(cb_id, Some("Processing\u{2026}"))
                .await?;

            if kind == "input" {
                bot.open_input_session(cid, raw_title);
                let questions = fetch_clarifier_questions(bot, item_id).await;
                let msg = if let Some(ref questions) = questions {
                    format!(
                        "\u{2753} <b>{title}</b>\n\n\
                         {}\n\n\
                         Reply with your answers, or send /input cancel to exit.",
                        mando_shared::escape_html(questions),
                    )
                } else {
                    format!(
                        "\u{1f9ed} Input: {title}\n\n\
                         Type extra context for this item, or send /input cancel to exit."
                    )
                };
                if let Err(e) = bot.edit_message(cid, mid, &msg).await {
                    tracing::warn!(module = "telegram", error = %e, "message send failed");
                }
            } else if kind == "reopen" {
                bot.pending_reopen.insert(
                    cid.to_string(),
                    (item_id.to_string(), raw_title.to_string()),
                );
                if let Err(e) = bot
                    .edit_message(
                        cid,
                        mid,
                        &format!(
                            "\u{1f504} Reopen: {title}\n\n\
                             Type your feedback \u{2014} what changes are needed?"
                        ),
                    )
                    .await
                {
                    tracing::warn!(module = "telegram", error = %e, "message send failed");
                }
            } else if kind == "rework" {
                bot.pending_rework.insert(
                    cid.to_string(),
                    (item_id.to_string(), raw_title.to_string()),
                );
                if let Err(e) = bot
                    .edit_message(
                        cid,
                        mid,
                        &format!(
                            "🔁 Rework: {title}\n\n\
                             Type the new instructions for Captain."
                        ),
                    )
                    .await
                {
                    tracing::warn!(module = "telegram", error = %e, "message send failed");
                }
            } else {
                if let Err(e) = bot
                    .edit_message(cid, mid, &format!("\u{23f3} {kind}: {title}\u{2026}"))
                    .await
                {
                    tracing::warn!(module = "telegram", error = %e, "message send failed");
                }
                execute_picker_action(bot, kind, cid, item_id, raw_title).await?;
            }
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

fn take_picker_by_kind(
    bot: &mut TelegramBot,
    kind: &str,
    aid: &str,
) -> Option<crate::bot::PickerState> {
    match kind {
        "input" => bot.take_input_picker(aid),
        "reopen" => bot.take_reopen_picker(aid),
        "rework" => bot.take_rework_picker(aid),
        "handoff" => bot.take_handoff_picker(aid),
        _ => None,
    }
}

/// Fetch the latest clarifier questions for a task from the gateway timeline.
async fn fetch_clarifier_questions(bot: &TelegramBot, item_id: &str) -> Option<String> {
    let path = format!("/api/tasks/{item_id}/timeline");
    let val = match bot.gw().get(&path).await {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(
                module = "telegram",
                item_id,
                error = %e,
                "failed to fetch timeline for clarifier questions"
            );
            return None;
        }
    };
    let events = val["events"].as_array()?;
    events
        .iter()
        .rev()
        .find(|e| e["event_type"].as_str() == Some("clarify_question"))
        .and_then(|e| {
            let q = &e["data"]["questions"];
            // Structured questions array: format into readable text.
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

async fn execute_picker_action(
    bot: &TelegramBot,
    kind: &str,
    cid: &str,
    item_id: &str,
    title: &str,
) -> Result<()> {
    match kind {
        "rework" => crate::callback_actions::rework(bot, cid, item_id, title).await,
        "handoff" => crate::callback_actions::handoff(bot, cid, item_id, title).await,
        _ => Ok(()),
    }
}
