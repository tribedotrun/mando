//! `/patterns` — review captain patterns and run the distiller.

use anyhow::Result;
use mando_shared::telegram_format::escape_html;
use serde_json::json;

use crate::bot::TelegramBot;

pub async fn handle(bot: &TelegramBot, chat_id: &str, args: &str) -> Result<()> {
    let mut parts = args.split_whitespace();
    let sub = parts.next().unwrap_or("list");

    match sub {
        "run" => match bot.gw().post("/api/knowledge/learn", &json!({})).await {
            Ok(resp) => {
                let summary = resp["summary"].as_str().unwrap_or("Distiller complete.");
                let found = resp["patterns_found"].as_i64().unwrap_or(0);
                bot.send_html(
                    chat_id,
                    &format!("🧪 {} Found {} pattern(s).", escape_html(summary), found),
                )
                .await?;
            }
            Err(e) => {
                bot.send_html(
                    chat_id,
                    &format!("❌ Distiller failed: {}", escape_html(&e.to_string())),
                )
                .await?;
            }
        },
        "approve" | "dismiss" => {
            let id = match parts.next().and_then(|value| value.parse::<i64>().ok()) {
                Some(id) => id,
                None => {
                    bot.send_html(
                        chat_id,
                        "Usage: /patterns approve &lt;id&gt; or /patterns dismiss &lt;id&gt;",
                    )
                    .await?;
                    return Ok(());
                }
            };
            let status = if sub == "approve" {
                "approved"
            } else {
                "dismissed"
            };
            match bot
                .gw()
                .post(
                    "/api/patterns/update",
                    &json!({ "id": id, "status": status }),
                )
                .await
            {
                Ok(_) => {
                    bot.send_html(chat_id, &format!("✅ Pattern #{id} marked {status}."))
                        .await?;
                }
                Err(e) => {
                    bot.send_html(
                        chat_id,
                        &format!("❌ Pattern update failed: {}", escape_html(&e.to_string())),
                    )
                    .await?;
                }
            }
        }
        _ => {
            let status = if sub == "list" {
                parts.next()
            } else {
                Some(sub)
            };
            let path = match status.filter(|value| !value.is_empty()) {
                Some(value) => format!("/api/patterns?status={value}"),
                None => "/api/patterns".to_string(),
            };
            match bot.gw().get(&path).await {
                Ok(resp) => {
                    let patterns = resp["patterns"].as_array().cloned().unwrap_or_default();
                    if patterns.is_empty() {
                        bot.send_html(chat_id, "🧠 No patterns found.").await?;
                        return Ok(());
                    }

                    let mut lines = vec![format!("🧠 <b>Patterns</b> ({})", patterns.len())];
                    for pattern in patterns.into_iter().take(10) {
                        let id = pattern["id"].as_i64().unwrap_or(0);
                        let label = pattern["pattern"].as_str().unwrap_or("?");
                        let recommendation = pattern["recommendation"]
                            .as_str()
                            .unwrap_or("No recommendation");
                        let status = pattern["status"].as_str().unwrap_or("pending");
                        lines.push(format!(
                            "\n• <b>#{id}</b> [{}] {}\n  {}",
                            escape_html(status),
                            escape_html(label),
                            escape_html(recommendation),
                        ));
                    }
                    bot.send_html(chat_id, &lines.join("\n")).await?;
                }
                Err(e) => {
                    bot.send_html(
                        chat_id,
                        &format!(
                            "❌ Failed to load patterns: {}",
                            escape_html(&e.to_string())
                        ),
                    )
                    .await?;
                }
            }
        }
    }

    Ok(())
}
