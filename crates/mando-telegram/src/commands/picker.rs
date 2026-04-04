//! Shared helpers for picker-based command handlers (cancel, delete, handoff,
//! reopen, rework).  Two flavours: **single-select** (pick one item) and
//! **multi-select** (toggle checkboxes, confirm batch).

use crate::bot::TelegramBot;
use anyhow::Result;
use mando_shared::telegram_format::escape_html;
use mando_types::Task;
use serde_json::json;

// ── Single-select picker ─────────────────────────────────────────────

/// Configuration for a single-select picker command.
pub struct SinglePicker {
    /// Header line shown above the item list.
    pub header: &'static str,
    /// Message shown when no eligible items exist.
    pub empty_msg: &'static str,
    /// Callback prefix, e.g. `"reopen"`. Produces `reopen:pick:{aid}:{idx}`.
    pub callback_prefix: &'static str,
    /// Max items to show (typically 8–10).
    pub limit: usize,
    /// Whether to show PR label after title (reopen/rework do, handoff doesn't).
    pub show_pr: bool,
    /// Format the button text given the 1-based display number.
    pub button_text: fn(usize) -> String,
    /// Which picker store to use on the bot.
    pub store: fn(&mut TelegramBot, &str, &str, &[&Task]),
}

/// Display a single-select picker: filter items → build numbered keyboard.
pub async fn show_single(
    bot: &mut TelegramBot,
    chat_id: &str,
    items: &[Task],
    cfg: &SinglePicker,
    filter: fn(&Task) -> bool,
) -> Result<()> {
    let eligible: Vec<_> = items
        .iter()
        .filter(|it| filter(it))
        .take(cfg.limit)
        .collect();

    if eligible.is_empty() {
        bot.send_html(chat_id, cfg.empty_msg).await?;
        return Ok(());
    }

    let action_id = super::short_uuid();
    let mut lines = vec![cfg.header.to_string()];
    let mut buttons: Vec<serde_json::Value> = Vec::new();

    for (idx, item) in eligible.iter().enumerate() {
        let num = idx + 1;
        let title = escape_html(&item.title);
        let pr_label = if cfg.show_pr {
            item.pr
                .as_deref()
                .map(|p| {
                    let link = mando_shared::helpers::pr_html_link(p, item.github_repo.as_deref());
                    format!(" ({link})")
                })
                .unwrap_or_default()
        } else {
            String::new()
        };
        lines.push(format!(" {num}. {title}{pr_label}"));
        buttons.push(json!([{
            "text": (cfg.button_text)(num),
            "callback_data": format!("{}:pick:{action_id}:{idx}", cfg.callback_prefix),
        }]));
    }
    buttons.push(json!([{
        "text": "Cancel",
        "callback_data": format!("{}:cancel:{action_id}", cfg.callback_prefix),
    }]));

    (cfg.store)(bot, &action_id, chat_id, &eligible);

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

// ── Multi-select picker ──────────────────────────────────────────────

/// Configuration for a multi-select picker command.
pub struct MultiPicker {
    /// Header line shown above the item list.
    pub header: &'static str,
    /// Message shown when no eligible items exist.
    pub empty_msg: &'static str,
    /// Callback prefix, e.g. `"ms_cancel"`. Produces `ms_cancel:toggle:{aid}:{idx}`.
    pub callback_prefix: &'static str,
    /// Label for the confirm button, e.g. "Cancel Selected".
    pub confirm_label: &'static str,
    /// Max items to show.
    pub limit: usize,
    /// Which picker store to use on the bot.
    pub store: fn(&mut TelegramBot, &str, &str, &[&Task]),
}

/// Display a multi-select picker: filter items → build checkbox keyboard with
/// confirm/dismiss buttons.
pub async fn show_multi(
    bot: &mut TelegramBot,
    chat_id: &str,
    items: &[Task],
    cfg: &MultiPicker,
    filter: fn(&Task) -> bool,
) -> Result<()> {
    let eligible: Vec<_> = items
        .iter()
        .filter(|it| filter(it))
        .take(cfg.limit)
        .collect();

    if eligible.is_empty() {
        bot.send_html(chat_id, cfg.empty_msg).await?;
        return Ok(());
    }

    let action_id = super::short_uuid();
    let mut lines = vec![cfg.header.to_string()];
    let mut buttons: Vec<serde_json::Value> = Vec::new();

    for (idx, item) in eligible.iter().enumerate() {
        let id = item.id;
        let title = escape_html(&item.title);
        lines.push(format!(" \u{25fb}\u{fe0f} #{id} {title}"));
        buttons.push(json!([{
            "text": format!("\u{25fb}\u{fe0f} #{id}"),
            "callback_data": format!("{}:toggle:{action_id}:{idx}", cfg.callback_prefix),
        }]));
    }
    buttons.push(json!([
        {"text": cfg.confirm_label, "callback_data": format!("{}:confirm:{action_id}", cfg.callback_prefix)},
        {"text": "Dismiss", "callback_data": format!("{}:cancel:{action_id}", cfg.callback_prefix)},
    ]));

    (cfg.store)(bot, &action_id, chat_id, &eligible);

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
