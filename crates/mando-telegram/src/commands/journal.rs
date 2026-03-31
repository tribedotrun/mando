//! `/journal [worker] [limit]` — show recent captain decisions.

use anyhow::Result;
use mando_shared::telegram_format::escape_html;

use crate::bot::TelegramBot;

pub async fn handle(bot: &TelegramBot, chat_id: &str, args: &str) -> Result<()> {
    let parts: Vec<&str> = args.split_whitespace().collect();
    let worker = parts.first().copied().filter(|s| !s.is_empty());
    let limit = parts
        .get(1)
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(10);

    let mut path = format!("/api/journal?limit={limit}");
    if let Some(worker_name) = worker {
        path.push_str("&worker=");
        path.push_str(worker_name);
    }

    match bot.gw().get(&path).await {
        Ok(resp) => {
            let totals = &resp["totals"];
            let total = totals["total"].as_i64().unwrap_or(0);
            let successes = totals["successes"].as_i64().unwrap_or(0);
            let failures = totals["failures"].as_i64().unwrap_or(0);
            let unresolved = totals["unresolved"].as_i64().unwrap_or(0);
            let mut lines = vec![format!(
                "🧠 <b>Captain journal</b>\n{} total · {} success · {} failure · {} pending",
                total, successes, failures, unresolved
            )];

            for decision in resp["decisions"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .into_iter()
                .take(limit)
            {
                let worker_name = decision["worker"].as_str().unwrap_or("?");
                let action = decision["action"].as_str().unwrap_or("?");
                let outcome = decision["outcome"].as_str().unwrap_or("pending");
                let rule = decision["rule"].as_str().unwrap_or("");
                let snippet: String = rule.chars().take(60).collect();
                lines.push(format!(
                    "\n• <code>{}</code> {} → {}\n  {}",
                    escape_html(worker_name),
                    escape_html(action),
                    escape_html(outcome),
                    escape_html(&snippet),
                ));
            }

            bot.send_html(chat_id, &lines.join("\n")).await?;
        }
        Err(e) => {
            bot.send_html(
                chat_id,
                &format!("❌ Failed to load journal: {}", escape_html(&e.to_string())),
            )
            .await?;
        }
    }

    Ok(())
}
