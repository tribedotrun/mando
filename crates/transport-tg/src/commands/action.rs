//! `/action` — unified task action picker.
//!
//! Replaces individual reopen, rework, accept, handoff, cancel, nudge, input,
//! answer, and ask commands with a single picker → state-aware action buttons.

use crate::telegram_format::escape_html;
use anyhow::Result;
use captain::ItemStatus;
use serde_json::json;

use crate::bot::TelegramBot;
use crate::commands;

pub(crate) use super::action_sessions::{
    fetch_clarifier_questions, handle_ask_text, handle_input_text,
};

// ── Command handler ─────────────────────────────────────────────────

/// Handle `/action [cancel]`.
pub async fn handle(bot: &mut TelegramBot, chat_id: &str, args: &str) -> Result<()> {
    let subcmd = args.trim().to_lowercase();

    if subcmd == "cancel" {
        let had_input = bot.has_input_session(chat_id);
        let had_ask = bot.has_ask_session(chat_id);
        if had_input {
            bot.close_input_session(chat_id);
        }
        if had_ask {
            if let Some(task_id) = bot.ask_session_task_id(chat_id) {
                let _ = bot
                    .gw()
                    .post("/api/tasks/ask/end", &json!({"id": task_id}))
                    .await;
            }
            bot.close_ask_session(chat_id);
        }
        if had_input || had_ask {
            bot.send_html(chat_id, "\u{2705} Session cancelled.")
                .await?;
        } else {
            bot.send_html(chat_id, "No active session.").await?;
        }
        return Ok(());
    }

    // Active session check
    if bot.has_input_session(chat_id) {
        let title = bot.input_session_title(chat_id).unwrap_or_default();
        bot.send_html(
            chat_id,
            &format!(
                "\u{1f9ed} Input session active: {}\n\
                 Reply with details, or /action cancel to exit.",
                escape_html(&title)
            ),
        )
        .await?;
        return Ok(());
    }
    if bot.has_ask_session(chat_id) {
        let rounds = bot.ask_session_rounds(chat_id);
        bot.send_html(
            chat_id,
            &format!(
                "Ask session active ({rounds} turns).\n\
                 Reply to continue, or /action cancel to exit."
            ),
        )
        .await?;
        return Ok(());
    }

    show_picker(bot, chat_id).await
}

// ── Picker ──────────────────────────────────────────────────────────

async fn show_picker(bot: &mut TelegramBot, chat_id: &str) -> Result<()> {
    let items = match commands::load_tasks_or_notify(bot, chat_id).await {
        Some(items) => items,
        None => return Ok(()),
    };

    if items.is_empty() {
        bot.send_html(chat_id, "No tasks found.").await?;
        return Ok(());
    }

    let mut sorted = items;
    sorted.sort_by(|a, b| {
        a.status
            .is_finalized()
            .cmp(&b.status.is_finalized())
            .then(b.id.cmp(&a.id))
    });

    let display: Vec<_> = sorted.iter().take(10).collect();
    let action_id = commands::short_uuid();

    bot.store_action_picker(&action_id, chat_id, &display);

    let mut lines = vec!["\u{2699}\u{fe0f} <b>Pick a task</b>\n".to_string()];
    let mut buttons: Vec<Vec<serde_json::Value>> = Vec::new();

    for (i, it) in display.iter().enumerate() {
        let status_tag = status_short(it.status);
        let title = commands::truncate(&it.title, 35);
        lines.push(format!(
            "{}. <b>#{}</b> {} {}",
            i + 1,
            it.id,
            escape_html(title),
            status_tag,
        ));
        buttons.push(vec![json!({
            "text": format!("#{} {}", it.id, status_tag),
            "callback_data": format!("act:pick:{action_id}:{i}"),
        })]);
    }

    // Two columns when many tasks
    let keyboard_buttons = if buttons.len() > 4 {
        buttons
            .chunks(2)
            .map(|chunk| chunk.iter().flatten().cloned().collect())
            .collect()
    } else {
        buttons
    };

    let mut kb_rows: Vec<Vec<serde_json::Value>> = keyboard_buttons;
    kb_rows.push(vec![json!({
        "text": "\u{274c} Cancel",
        "callback_data": format!("act:cancel:{action_id}"),
    })]);

    bot.api()
        .send_message(
            chat_id,
            &lines.join("\n"),
            Some("HTML"),
            Some(json!({"inline_keyboard": kb_rows})),
            true,
        )
        .await?;

    Ok(())
}

// ── Action buttons per state ────────────────────────────────────────

/// Build action buttons for a task based on its status.
pub(crate) fn action_buttons(
    task_id: &str,
    status: ItemStatus,
    has_pr: bool,
) -> Vec<Vec<serde_json::Value>> {
    let mut actions: Vec<(&str, &str)> = Vec::new();

    match status {
        ItemStatus::Clarifying => {
            actions.push(("\u{1f4ac} Ask", "ask"));
            actions.push(("\u{270d}\u{fe0f} Input", "input"));
            actions.push(("\u{274c} Cancel", "cancel"));
        }
        ItemStatus::New | ItemStatus::NeedsClarification | ItemStatus::Queued => {
            actions.push(("\u{270d}\u{fe0f} Input", "input"));
            actions.push(("\u{274c} Cancel", "cancel"));
        }
        ItemStatus::InProgress => {
            actions.push(("\u{1f4ac} Ask", "ask"));
            actions.push(("\u{1f4e3} Nudge", "nudge"));
            actions.push(("\u{1f91d} Handoff", "handoff"));
            actions.push(("\u{274c} Cancel", "cancel"));
        }
        ItemStatus::CaptainReviewing | ItemStatus::CaptainMerging => {
            actions.push(("\u{1f4ac} Ask", "ask"));
            actions.push(("\u{274c} Cancel", "cancel"));
        }
        ItemStatus::Rework => {
            actions.push(("\u{274c} Cancel", "cancel"));
        }
        ItemStatus::AwaitingReview => {
            actions.push(("\u{1f4ac} Ask", "ask"));
            if has_pr {
                actions.push(("\u{1f500} Merge", "merge"));
            } else {
                actions.push(("\u{2705} Accept", "accept"));
            }
            actions.push(("\u{1f91d} Handoff", "handoff"));
            actions.push(("\u{274c} Cancel", "cancel"));
        }
        ItemStatus::Escalated | ItemStatus::Errored => {
            actions.push(("\u{1f4ac} Ask", "ask"));
            actions.push(("\u{1f504} Reopen", "reopen"));
            actions.push(("\u{1f501} Rework", "rework"));
            actions.push(("\u{274c} Cancel", "cancel"));
        }
        ItemStatus::HandedOff => {
            actions.push(("\u{1f504} Reopen", "reopen"));
            actions.push(("\u{1f501} Rework", "rework"));
        }
        ItemStatus::Merged | ItemStatus::CompletedNoPr => {
            actions.push(("\u{1f4ac} Ask", "ask"));
            actions.push(("\u{1f504} Reopen", "reopen"));
            actions.push(("\u{1f501} Rework", "rework"));
        }
        ItemStatus::PlanReady => {
            actions.push(("\u{1f680} Implement", "input"));
            actions.push(("\u{274c} Cancel", "cancel"));
        }
        ItemStatus::Canceled => {
            actions.push(("\u{1f504} Reopen", "reopen"));
            actions.push(("\u{1f501} Rework", "rework"));
        }
    }

    if actions.is_empty() {
        return vec![vec![json!({
            "text": "No actions available",
            "callback_data": "act:noop",
        })]];
    }

    actions
        .chunks(3)
        .map(|chunk| {
            chunk
                .iter()
                .map(|(label, action)| {
                    json!({
                        "text": label,
                        "callback_data": format!("act:do:{task_id}:{action}"),
                    })
                })
                .collect()
        })
        .collect()
}

pub(crate) fn status_short(s: ItemStatus) -> &'static str {
    match s {
        ItemStatus::New => "[new]",
        ItemStatus::Clarifying => "[clarifying]",
        ItemStatus::NeedsClarification => "[needs-input]",
        ItemStatus::Queued => "[queued]",
        ItemStatus::InProgress => "[working]",
        ItemStatus::CaptainReviewing => "[reviewing]",
        ItemStatus::CaptainMerging => "[merging]",
        ItemStatus::AwaitingReview => "[review]",
        ItemStatus::Rework => "[rework]",
        ItemStatus::HandedOff => "[handed-off]",
        ItemStatus::Escalated => "[escalated]",
        ItemStatus::Errored => "[errored]",
        ItemStatus::Merged => "[merged]",
        ItemStatus::CompletedNoPr => "[done]",
        ItemStatus::PlanReady => "[plan-ready]",
        ItemStatus::Canceled => "[canceled]",
    }
}

// Session text handlers (input, ask, clarifier fetch) are in action_sessions.rs.
