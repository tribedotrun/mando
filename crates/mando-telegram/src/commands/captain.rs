//! `/captain [dry-run]` — trigger captain tick manually.

use crate::bot::TelegramBot;
use anyhow::Result;
use mando_shared::telegram_format::escape_html;
use serde_json::json;

/// Handle `/captain [dry-run]`.
pub async fn handle(bot: &TelegramBot, chat_id: &str, args: &str) -> Result<()> {
    let subcmd = args.trim().to_lowercase();
    let is_dry_run = matches!(subcmd.as_str(), "dry-run" | "dryrun" | "dry");

    let mode = if is_dry_run { "dry-run" } else { "live" };
    let ack = bot
        .send_html(
            chat_id,
            &format!("\u{2699}\u{fe0f} Running captain tick ({mode})\u{2026}"),
        )
        .await?;
    let ack_mid = ack.get("message_id").and_then(|v| v.as_i64()).unwrap_or(0);

    match bot
        .gw()
        .post(
            "/api/captain/tick",
            &json!({"dry_run": is_dry_run, "emit_notifications": false}),
        )
        .await
    {
        Ok(tick) => {
            let mut lines = vec![format!(
                "\u{2705} <b>Captain tick complete ({})</b>",
                escape_html(tick["mode"].as_str().unwrap_or(mode))
            )];

            // Per-action breakdown
            if let Some(actions) = tick["dry_actions"].as_array() {
                for action in actions {
                    let kind = action["action"].as_str().unwrap_or("Skip");
                    let worker = action["worker"].as_str().unwrap_or("?");
                    let reason = action["reason"].as_str().unwrap_or("");
                    let verb = match kind {
                        "nudge" => "Dispatched",
                        "skip" => "Skipped",
                        "ship" => "Done",
                        "captain-review" => "Reviewing",
                        _ => kind,
                    };
                    if reason.is_empty() {
                        lines.push(format!("{verb}: {}", escape_html(worker)));
                    } else {
                        lines.push(format!(
                            "{verb}: {} \u{2014} {}",
                            escape_html(worker),
                            escape_html(reason)
                        ));
                    }
                }
            }

            lines.push(format!(
                "\u{1f477} Workers: {}/{}",
                tick["active_workers"].as_i64().unwrap_or(0),
                tick["max_workers"].as_i64().unwrap_or(0)
            ));

            if let Some(tasks) = tick["tasks"].as_object() {
                let mut counts: Vec<String> = tasks
                    .iter()
                    .map(|(k, v)| format!("{}={}", k, v.as_i64().unwrap_or(0)))
                    .collect();
                counts.sort();
                lines.push(format!("\u{1f4cb} Tasks: {}", counts.join(", ")));
            }

            if let Some(alerts) = tick["alerts"].as_array() {
                if !alerts.is_empty() {
                    lines.push(format!("\u{26a0}\u{fe0f} Alerts: {}", alerts.len()));
                    for alert in alerts.iter().take(3) {
                        lines.push(format!(
                            "  \u{2022} {}",
                            escape_html(alert.as_str().unwrap_or(""))
                        ));
                    }
                }
            }

            if let Some(err) = tick["error"].as_str() {
                lines.push(format!("\u{274c} Error: {}", escape_html(err)));
            }

            bot.edit_message(chat_id, ack_mid, &lines.join("\n"))
                .await?;
        }
        Err(e) => {
            bot.edit_message(
                chat_id,
                ack_mid,
                &format!(
                    "\u{274c} Captain tick failed: {}",
                    escape_html(&e.to_string())
                ),
            )
            .await?;
        }
    }
    Ok(())
}
