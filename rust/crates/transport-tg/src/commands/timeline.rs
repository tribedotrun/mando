//! `/timeline [id] [chat]` — full lifecycle timeline for a task.

use crate::bot::TelegramBot;
use crate::gateway_paths as paths;
use crate::telegram_format::{escape_html, render_markdown_reply_html};
use anyhow::Result;
use tracing::warn;

/// Visible-text budget for an LLM-authored timeline event summary line.
const EVENT_SUMMARY_VISIBLE_BUDGET: usize = 80;

/// Visible-text budget for an LLM-authored Q&A history entry line.
const QA_ENTRY_VISIBLE_BUDGET: usize = 120;

/// Build a single timeline event line: ts code-span, icon, escaped event
/// kind, then the renderer-formatted summary (LLM markdown).
fn format_event_line(short_ts: &str, kind: &str, icon: &str, summary_markdown: &str) -> String {
    format!(
        "<code>{}</code> {} <b>{}</b> {}",
        escape_html(short_ts),
        icon,
        escape_html(kind),
        render_markdown_reply_html(summary_markdown, EVENT_SUMMARY_VISIBLE_BUDGET),
    )
}

/// Build a single Q&A history line: role icon plus renderer-formatted
/// entry content (LLM-authored answer markdown).
fn format_qa_history_line(icon: &str, content_markdown: &str) -> String {
    format!(
        "{} {}",
        icon,
        render_markdown_reply_html(content_markdown, QA_ENTRY_VISIBLE_BUDGET),
    )
}

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
///
/// If no args, sets pending state so the next plain-text message is treated
/// as the timeline args. Otherwise runs `execute` directly.
pub async fn handle(bot: &TelegramBot, chat_id: &str, args: &str) -> Result<()> {
    if args.trim().is_empty() {
        // Send the prompt first so we can capture its message_id; the
        // pending entry stores that id so a quote-reply routes back here.
        let prompt = bot
            .send_html(
                chat_id,
                "Send the task ID below (optionally followed by <code>chat</code> for Q&amp;A only).\nReply to this prompt to disambiguate.",
            )
            .await?;
        let prompt_mid = prompt
            .get("message_id")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        bot.set_pending_timeline(chat_id, prompt_mid).await;
        return Ok(());
    }

    bot.take_pending_timeline(chat_id).await;
    execute(bot, chat_id, args).await
}

/// Render the timeline for a task. Reachable from inline args and from the
/// drained `pending_timeline` follow-up text.
pub async fn execute(bot: &TelegramBot, chat_id: &str, args: &str) -> Result<()> {
    let parts: Vec<&str> = args.split_whitespace().collect();
    if parts.is_empty() {
        bot.send_html(
            chat_id,
            "\u{26a0}\u{fe0f} No task ID provided. Try <code>/timeline &lt;id&gt;</code>.",
        )
        .await?;
        return Ok(());
    }

    let item_id = parts[0];
    let chat_only = parts.get(1).is_some_and(|s| s.eq_ignore_ascii_case("chat"));

    // The gateway timeline endpoint returns all events (ignores query params).
    // We filter locally when `chat` mode is requested.
    let path = paths::task_timeline(item_id);

    match bot
        .gw()
        .get_typed::<api_types::TimelineResponse>(&path)
        .await
    {
        Ok(val) => {
            let all_events = val.events;
            let events: Vec<api_types::TimelineEvent> = if chat_only {
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
                    let ts = event.timestamp.as_str();
                    let kind = event.data.event_type_str();
                    let detail = event.summary.as_str();
                    let short_ts = super::truncate(ts, 16);
                    let icon = timeline_icon(kind);
                    lines.push(format_event_line(short_ts, kind, icon, detail));
                }

                if events.len() > 20 {
                    lines.push(format!("\n\u{2026} and {} more events", events.len() - 20));
                }

                // Q&A history
                if let Ok(hist_resp) = bot
                    .gw()
                    .get_typed::<api_types::AskHistoryResponse>(&paths::task_history(item_id))
                    .await
                {
                    if !hist_resp.history.is_empty() {
                        lines.push("\n\u{1f4ac} <b>Q&A History</b>".to_string());
                        for entry in hist_resp.history.iter().take(5) {
                            let icon = if entry.role == "human" {
                                "\u{1f464}"
                            } else {
                                "\u{1f916}"
                            };
                            lines.push(format_qa_history_line(icon, &entry.content));
                        }
                        if hist_resp.history.len() > 5 {
                            lines
                                .push(format!("\u{2026} and {} more", hist_resp.history.len() - 5));
                        }
                    }
                }

                // Try HTML first, fall back to plain text if parse fails
                let html_text = lines.join("\n");
                match bot.send_html(chat_id, &html_text).await {
                    Ok(_) => {}
                    Err(e) => {
                        warn!(module = "timeline", error = %e, "HTML send failed, falling back to plain text");
                        let mut plain_lines = vec![format!("{mode} Timeline for #{item_id}\n")];
                        for event in events.iter().take(20) {
                            let ts = event.timestamp.as_str();
                            let kind = event.data.event_type_str();
                            let detail = event.summary.as_str();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_line_renders_inline_code_in_summary() {
        let line = format_event_line(
            "2026-04-29T00:00",
            "worker_completed",
            "\u{2705}",
            "Done — `cargo test` passed",
        );

        assert!(line.contains("<code>cargo test</code>"), "line: {line}");
        assert!(!line.contains("`cargo test`"), "literal backticks: {line}");
        assert!(line.contains("<b>worker_completed</b>"));
    }

    #[test]
    fn event_line_renders_bold_in_summary() {
        let line = format_event_line("ts", "kind", "icon", "made it **really** fast");
        assert!(line.contains("<b>really</b>"));
        assert!(!line.contains("**"));
    }

    #[test]
    fn qa_history_line_renders_markdown_answer() {
        let line = format_qa_history_line("\u{1f916}", "## Summary\n- step one\n- step `two`");

        assert!(line.contains("Summary"));
        assert!(!line.contains("##"));
        assert!(line.contains("\u{2022} step one"));
        assert!(line.contains("<code>two</code>"));
    }

    #[test]
    fn qa_history_line_starts_with_role_icon() {
        let line = format_qa_history_line("\u{1f464}", "human question");
        assert!(line.starts_with("\u{1f464} "));
    }
}
