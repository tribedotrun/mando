//! `/triage` — rank pending-review PRs by merge readiness via gateway API.

use anyhow::Result;
use serde_json::json;

use mando_shared::telegram_format::escape_html;

use crate::bot::TelegramBot;
use crate::gateway_paths as paths;

/// Handle `/triage`.
pub async fn handle(bot: &TelegramBot, chat_id: &str, _args: &str) -> Result<()> {
    let ack = bot
        .send_html(chat_id, "Triaging pending-review PRs\u{2026}")
        .await?;
    let ack_mid = ack.get("message_id").and_then(|v| v.as_i64()).unwrap_or(0);

    match bot.gw().post(paths::CAPTAIN_TRIAGE, &json!({})).await {
        Ok(resp) => {
            let items = resp["items"].as_array().cloned().unwrap_or_default();
            if items.is_empty() {
                let msg = resp["message"]
                    .as_str()
                    .unwrap_or("No pending-review PRs to triage.");
                bot.edit_message(chat_id, ack_mid, msg).await?;
                return Ok(());
            }

            // Format triage results for Telegram
            let mut lines = Vec::new();
            lines.push(format!("<b>{} Pending-Review Tasks</b>\n", items.len()));

            for (i, item) in items.iter().enumerate() {
                let repo = item["repo"].as_str().unwrap_or("?");
                let repo_short = repo.split('/').next_back().unwrap_or(repo);
                let pr_num = item["pr_number"].as_i64().unwrap_or(0);
                let title = item["title"].as_str().unwrap_or("?");
                let fast_track = item["fast_track"].as_bool() == Some(true);
                let fetch_failed = item["fetch_failed"].as_bool() == Some(true);
                let file_count = item["file_count"].as_i64().unwrap_or(0);
                let score = item["merge_readiness_score"].as_i64().unwrap_or(0);

                let tag = if fetch_failed {
                    "[FETCH FAILED]"
                } else if fast_track {
                    "[Fast-Track]"
                } else {
                    ""
                };
                let risk = item["cursor_risk"]
                    .as_str()
                    .map(|r| format!("Risk: {r}"))
                    .unwrap_or_default();
                let files = if fetch_failed {
                    "?".to_string()
                } else {
                    file_count.to_string()
                };
                let truncated = super::truncate(title, 45);
                let display_title = if truncated.len() < title.len() {
                    format!("{truncated}\u{2026}")
                } else {
                    title.to_string()
                };

                lines.push(format!(
                    "{}. <b>{repo_short}</b> #{pr_num} {tag}\n   {}\n   {files} files | score {score} {risk}",
                    i + 1,
                    escape_html(&display_title),
                ));
            }

            bot.edit_message(chat_id, ack_mid, &lines.join("\n"))
                .await?;
        }
        Err(e) => {
            bot.edit_message(
                chat_id,
                ack_mid,
                &format!("\u{274c} Triage failed: {}", escape_html(&e.to_string())),
            )
            .await?;
        }
    }
    Ok(())
}
