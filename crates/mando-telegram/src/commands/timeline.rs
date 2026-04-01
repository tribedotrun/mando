//! `/timeline [id] [chat]` — full lifecycle timeline for a task.

use crate::bot::TelegramBot;
use anyhow::Result;
use mando_shared::telegram_format::escape_html;
use tracing::warn;

/// Map timeline event kind to an emoji icon.
fn timeline_icon(kind: &str) -> &'static str {
    match kind {
        "created" => "\u{2795}",                              // plus
        "clarify_started" | "clarify_question" => "\u{2753}", // question mark
        "clarify_resolved" => "\u{2705}",                     // check mark
        "human_answered" => "\u{1f4ac}",                      // speech bubble
        "worker_spawned" => "\u{1f680}",                      // rocket
        "worker_nudged" => "\u{1f4a5}",                       // collision
        "session_resumed" => "\u{1f504}",                     // counterclockwise
        "worker_completed" => "\u{2705}",                     // check mark
        "captain_review_started" => "\u{1f9d0}",              // monocle face
        "captain_review_verdict" => "\u{2696}\u{fe0f}",       // scales
        "awaiting_review" => "\u{1f440}",                     // eyes
        "human_reopen" => "\u{1f504}",                        // counterclockwise
        "human_ask" => "\u{2753}",                            // question mark
        "rebase_triggered" => "\u{1f500}",                    // shuffle
        "rework_requested" => "\u{1f504}",                    // counterclockwise
        "merged" => "\u{1f389}",                              // party popper
        "escalated" => "\u{1f6a8}",                           // rotating light
        "errored" => "\u{26a0}\u{fe0f}",                      // warning
        "canceled" => "\u{274c}",                             // cross mark
        "handed_off" => "\u{1f91d}",                          // handshake
        "status_changed" => "\u{1f504}",                      // counterclockwise
        _ => "\u{2022}",                                      // bullet
    }
}

/// Handle `/timeline [id] [chat]`.
pub async fn handle(bot: &TelegramBot, chat_id: &str, args: &str) -> Result<()> {
    let parts: Vec<&str> = args.split_whitespace().collect();
    if parts.is_empty() {
        bot.send_html(chat_id, "Usage: /timeline &lt;task_id&gt; [chat]")
            .await?;
        return Ok(());
    }

    let item_id = parts[0];
    let chat_only = parts.get(1).is_some_and(|s| s.eq_ignore_ascii_case("chat"));

    // The gateway timeline endpoint returns all events (ignores query params).
    // We filter locally when `chat` mode is requested.
    let path = format!("/api/tasks/{}/timeline", item_id);

    match bot.gw().get(&path).await {
        Ok(val) => {
            let all_events = val["events"].as_array().cloned().unwrap_or_default();
            let events: Vec<serde_json::Value> = if chat_only {
                // In chat mode, take only the last 10 events.
                all_events.into_iter().rev().take(10).rev().collect()
            } else {
                all_events
            };
            if events.is_empty() {
                bot.send_html(
                    chat_id,
                    &format!(
                        "\u{1f4c5} <b>Timeline for #{}</b>\n\nNo events found.",
                        escape_html(item_id)
                    ),
                )
                .await?;
            } else {
                let mode = if chat_only { "Q&A" } else { "Full" };
                let mut lines = vec![format!(
                    "\u{1f4c5} <b>{mode} Timeline for #{}</b>\n",
                    escape_html(item_id)
                )];

                for event in events.iter().take(20) {
                    let ts = event["ts"].as_str().unwrap_or("");
                    let kind = event["kind"].as_str().unwrap_or("event");
                    let detail = event["detail"].as_str().unwrap_or("");
                    let short_ts = super::truncate(ts, 16);
                    let icon = timeline_icon(kind);
                    lines.push(format!(
                        "<code>{}</code> {} <b>{}</b> {}",
                        escape_html(short_ts),
                        icon,
                        escape_html(kind),
                        escape_html(super::truncate(detail, 80)),
                    ));
                }

                if events.len() > 20 {
                    lines.push(format!("\n\u{2026} and {} more events", events.len() - 20));
                }

                // Try HTML first, fall back to plain text if parse fails
                let html_text = lines.join("\n");
                match bot.send_html(chat_id, &html_text).await {
                    Ok(_) => {}
                    Err(e) => {
                        warn!(module = "timeline", error = %e, "HTML send failed, falling back to plain text");
                        let mut plain_lines = vec![format!("{mode} Timeline for #{item_id}\n")];
                        for event in events.iter().take(20) {
                            let ts = event["ts"].as_str().unwrap_or("");
                            let kind = event["kind"].as_str().unwrap_or("event");
                            let detail = event["detail"].as_str().unwrap_or("");
                            plain_lines.push(format!(
                                "{} | {kind} {}",
                                super::truncate(ts, 16),
                                super::truncate(detail, 80),
                            ));
                        }
                        bot.api()
                            .send_message(chat_id, &plain_lines.join("\n"), None, None, true)
                            .await?;
                    }
                }
            }
        }
        Err(e) => {
            bot.send_html(
                chat_id,
                &format!(
                    "\u{274c} Failed to load timeline: {}",
                    escape_html(&e.to_string())
                ),
            )
            .await?;
        }
    }
    Ok(())
}
