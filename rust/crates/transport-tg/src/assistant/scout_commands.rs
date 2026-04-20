//! Scout-related command handlers for the assistant bot.

use anyhow::Result;

use crate::telegram_format::escape_html;

use super::commands::send_html;
use super::formatting::{format_swipe_card, swipe_card_kb};
use crate::bot::TelegramBot;
use crate::gateway_paths as paths;

// ── /scout (swipe flow only) ────────────────────────────────────────

pub async fn cmd_scout(bot: &mut TelegramBot, chat_id: &str) -> Result<()> {
    swipe_start(bot, chat_id).await
}

// ── Swipe flow ──────────────────────────────────────────────────────

async fn swipe_start(bot: &TelegramBot, chat_id: &str) -> Result<()> {
    let result = match bot
        .gw()
        .get_typed::<api_types::ScoutResponse>(&paths::processed_scout_items(10000))
        .await
    {
        Ok(r) => r,
        Err(e) => {
            send_html(
                bot,
                chat_id,
                &format!(
                    "\u{274c} Failed to load scout: {}",
                    escape_html(&e.to_string())
                ),
            )
            .await?;
            return Ok(());
        }
    };
    let first_id = result.items.first().map(|item| item.id);

    match first_id {
        Some(id) => show_card(bot, chat_id, id).await,
        None => {
            send_html(
                bot,
                chat_id,
                "\u{1f4ed} Inbox zero \u{2014} no processed items to review.",
            )
            .await?;
            Ok(())
        }
    }
}

pub async fn show_card(bot: &TelegramBot, chat_id: &str, id: i64) -> Result<()> {
    let item = bot
        .gw()
        .get_typed::<api_types::ScoutItem>(&paths::scout_item(id))
        .await?;
    let item_val = serde_json::to_value(&item).unwrap_or_default();
    let summary = item.summary.as_deref();
    let text = format_swipe_card(&item_val, summary);
    let tg_url = item.telegraph_url.as_deref();
    let kb = swipe_card_kb(id, tg_url);

    bot.api
        .send_message(chat_id, &text, Some("HTML"), Some(kb), true)
        .await?;
    Ok(())
}

// ── /scout_research ────────────────────────────────────────────────

pub async fn cmd_research(bot: &mut TelegramBot, chat_id: &str, args: &str) -> Result<()> {
    let topic = args.trim();
    if topic.is_empty() {
        send_html(
            bot,
            chat_id,
            "Usage: /scout_research &lt;topic&gt;\nExample: /scout_research Rust async patterns",
        )
        .await?;
        return Ok(());
    }

    let sent = send_html(
        bot,
        chat_id,
        &format!(
            "\u{1f50d} Researching <b>{}</b>\u{2026}",
            escape_html(topic)
        ),
    )
    .await?;
    let message_id = sent["message_id"].as_i64().unwrap_or(0);

    let body = serde_json::json!({"topic": topic, "process": true});
    let post_result = bot
        .gw()
        .post_typed::<_, api_types::ResearchStartResponse>(paths::SCOUT_RESEARCH, &body)
        .await;

    let error_text: Option<String> = match post_result {
        Ok(result) => {
            let run_id = result.run_id;
            if run_id > 0 && message_id > 0 {
                // Register the "Researching..." message for SSE-driven updates.
                let key = format!("research:{run_id}");
                let mut guard = match bot.pending_scout_msgs.lock() {
                    Ok(g) => g,
                    Err(poisoned) => {
                        tracing::warn!(
                            target: "transport-tg",
                            module = "transport-tg",
                            "pending_scout_msgs mutex poisoned; recovering",
                        );
                        poisoned.into_inner()
                    }
                };
                guard.insert(key, message_id);
                None
            } else {
                Some(
                    "\u{274c} Research failed: invalid daemon response (missing run_id)"
                        .to_string(),
                )
            }
        }
        Err(e) => Some(format!(
            "\u{274c} Research failed: {}",
            escape_html(&e.to_string())
        )),
    };

    if let Some(text) = error_text {
        if message_id > 0 {
            if let Err(edit_err) = bot
                .api
                .edit_message_text(chat_id, message_id, &text, Some("HTML"), None)
                .await
            {
                tracing::warn!(module = "transport-tg-assistant-scout_commands", error = %edit_err, "edit failed, sending new message");
                global_infra::best_effort!(
                    bot.api
                        .send_message(chat_id, &text, Some("HTML"), None, true)
                        .await,
                    "scout_commands: bot .api .send_message(chat_id, &text, Some('HTML'), None, t"
                );
            }
        }
    }
    Ok(())
}
