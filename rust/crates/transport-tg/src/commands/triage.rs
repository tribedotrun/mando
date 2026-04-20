//! `/triage` — rank pending-review PRs by merge readiness via gateway API.

use anyhow::Result;

use crate::telegram_format::escape_html;

use crate::bot::TelegramBot;
use crate::gateway_paths as paths;

/// Handle `/triage`.
pub async fn handle(bot: &TelegramBot, chat_id: &str, _args: &str) -> Result<()> {
    let ack = bot
        .send_html(chat_id, "Triaging pending-review PRs\u{2026}")
        .await?;
    let ack_mid = ack.get("message_id").and_then(|v| v.as_i64()).unwrap_or(0);

    match bot
        .gw()
        .post_typed::<api_types::TriageRequest, api_types::TriageResponse>(
            paths::CAPTAIN_TRIAGE,
            &api_types::TriageRequest { item_id: None },
        )
        .await
    {
        Ok(resp) => {
            if resp.items.is_empty() {
                bot.edit_message(chat_id, ack_mid, "No pending-review PRs to triage.")
                    .await?;
                return Ok(());
            }

            // Format triage results for Telegram
            let mut lines = Vec::new();
            lines.push(format!(
                "<b>{} Pending-Review Tasks</b>\n",
                resp.items.len()
            ));

            for (i, item) in resp.items.iter().enumerate() {
                let repo_short = item
                    .github_repo
                    .split('/')
                    .next_back()
                    .unwrap_or(item.github_repo.as_str());
                let tag = if item.fetch_failed {
                    "[FETCH FAILED]"
                } else if item.fast_track {
                    "[Fast-Track]"
                } else {
                    ""
                };
                let risk = item
                    .cursor_risk
                    .as_ref()
                    .map(|r| format!("Risk: {r}"))
                    .unwrap_or_default();
                let files = if item.fetch_failed {
                    "?".to_string()
                } else {
                    item.file_count.to_string()
                };
                let score = item.merge_readiness_score;
                let truncated = super::truncate(&item.title, 45);
                let display_title = if truncated.len() < item.title.len() {
                    format!("{truncated}\u{2026}")
                } else {
                    item.title.clone()
                };

                lines.push(format!(
                    "{}. <b>{repo_short}</b> #{} {tag}\n   {}\n   {files} files | score {score:.0} {risk}",
                    i + 1,
                    item.pr_number,
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
