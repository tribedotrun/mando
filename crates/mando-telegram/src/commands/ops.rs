//! `/ops [new|message]` — multi-turn ops copilot via HTTP to gateway.

use crate::bot::TelegramBot;
use anyhow::Result;
use mando_shared::{render_markdown_reply_html, TELEGRAM_TEXT_MAX_LEN};
use serde_json::json;

const OPS_PREFIX: &str = "\u{1f527} ";

fn ops_kb() -> serde_json::Value {
    json!({
        "inline_keyboard": [[
            {"text": "New session", "callback_data": "ops:new"},
            {"text": "End session", "callback_data": "ops:end"},
        ]]
    })
}

/// Handle `/ops [new|message]`.
pub async fn handle(bot: &mut TelegramBot, chat_id: &str, args: &str) -> Result<()> {
    let text = args.trim();

    if text.eq_ignore_ascii_case("new") {
        // End any existing server-side session before starting fresh.
        let key = format!("ops:{chat_id}");
        bot.gw()
            .post("/api/ops/end", &json!({"key": key}))
            .await
            .ok();
        bot.close_ops_session(chat_id);
        bot.open_ops_session(chat_id);
        bot.api()
            .send_message(
                chat_id,
                "\u{1f527} New ops session. What do you need?",
                None,
                Some(ops_kb()),
                true,
            )
            .await?;
        return Ok(());
    }

    if text.is_empty() {
        if bot.has_ops_session(chat_id) {
            let rounds = bot.ops_session_rounds(chat_id);
            bot.api()
                .send_message(
                    chat_id,
                    &format!(
                        "\u{1f527} Ops session active ({rounds} turns). \
                         Reply to continue, or /ops new for a fresh session."
                    ),
                    None,
                    Some(ops_kb()),
                    true,
                )
                .await?;
        } else {
            bot.send_html(
                chat_id,
                "\u{1f527} No active ops session. Use /ops &lt;message&gt; to start one.",
            )
            .await?;
        }
        return Ok(());
    }

    ops_turn(bot, chat_id, text).await
}

/// Execute one ops copilot turn via HTTP to gateway.
pub async fn ops_turn(bot: &mut TelegramBot, chat_id: &str, text: &str) -> Result<()> {
    if !bot.has_ops_session(chat_id) {
        bot.open_ops_session(chat_id);
    }
    bot.increment_ops_rounds(chat_id);

    let ack = bot
        .send_html(chat_id, "\u{1f527} Working on it\u{2026}")
        .await?;
    let ack_mid = ack.get("message_id").and_then(|v| v.as_i64()).unwrap_or(0);

    let key = format!("ops:{chat_id}");
    let prompt = format!(
        "You are an ops copilot for a software development team. \
         Help with the following request. Be concise.\n\n{}",
        text
    );

    // Try follow-up first; if no session exists, start a new one.
    let response = match bot
        .gw()
        .post("/api/ops/message", &json!({"key": key, "message": prompt}))
        .await
    {
        Ok(resp) => super::extract_result_text(&resp),
        Err(_) => {
            // No existing session — start fresh.
            match bot
                .gw()
                .post("/api/ops/start", &json!({"key": key, "prompt": prompt}))
                .await
            {
                Ok(resp) => super::extract_result_text(&resp),
                Err(e) => format!("\u{274c} Ops CC failed: {e}"),
            }
        }
    };

    let visible_budget = TELEGRAM_TEXT_MAX_LEN.saturating_sub(OPS_PREFIX.len());
    let display = render_markdown_reply_html(&response, visible_budget);

    bot.edit_message_with_markup(
        chat_id,
        ack_mid,
        &format!("{OPS_PREFIX}{display}"),
        Some(ops_kb()),
    )
    .await?;

    Ok(())
}

/// Handle text messages routed to ops session.
pub async fn handle_text(bot: &mut TelegramBot, chat_id: &str, text: &str) -> Result<bool> {
    if !bot.has_ops_session(chat_id) {
        return Ok(false);
    }
    ops_turn(bot, chat_id, text).await?;
    Ok(true)
}
