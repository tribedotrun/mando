//! `/sessions` — list recent CC sessions via gateway API.

use crate::bot::TelegramBot;
use anyhow::Result;
use mando_shared::telegram_format::escape_html;

/// Handle `/sessions`.
pub async fn handle(bot: &TelegramBot, chat_id: &str, _args: &str) -> Result<()> {
    let resp = match bot.gw().get("/api/sessions?per_page=10").await {
        Ok(r) => r,
        Err(e) => {
            bot.send_html(
                chat_id,
                &format!(
                    "\u{274c} Failed to load sessions: {}",
                    escape_html(&e.to_string())
                ),
            )
            .await?;
            return Ok(());
        }
    };

    let sessions = resp["sessions"].as_array().cloned().unwrap_or_default();

    if sessions.is_empty() {
        bot.send_html(
            chat_id,
            "\u{1f4cb} <b>Recent Sessions</b>\n\nNo sessions recorded yet.",
        )
        .await?;
        return Ok(());
    }

    let mut lines = vec![format!(
        "\u{1f4cb} <b>Recent Sessions</b> (last {})\n",
        sessions.len()
    )];

    for s in &sessions {
        let sid = s["session_id"].as_str().unwrap_or("?");
        let short_id = super::truncate(sid, 8);
        let caller = s["caller"].as_str().unwrap_or("?");
        let ts = s["started_at"].as_str().unwrap_or("");
        let short_ts = super::truncate(ts, 16);
        let cost = s["cost_usd"].as_f64();
        let dur = s["duration_ms"].as_u64();
        let resumed = s["resumed"].as_bool() == Some(true);

        let mut detail = format!(
            "<code>{}</code> {} <b>{}</b>",
            escape_html(short_ts),
            if resumed { "\u{1f504}" } else { "\u{1f195}" },
            escape_html(caller),
        );

        if let Some(d) = dur {
            detail.push_str(&format!(" {}s", d / 1000));
        }
        if let Some(c) = cost {
            detail.push_str(&format!(" ${:.2}", c));
        }

        detail.push_str(&format!(" <code>{}</code>", escape_html(short_id)));
        lines.push(detail);
    }

    bot.send_html(chat_id, &lines.join("\n")).await?;
    Ok(())
}
