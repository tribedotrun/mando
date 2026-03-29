//! `/status [all]` — task overview grouped by repo → status, with merge/accept buttons.

use crate::bot::TelegramBot;
use anyhow::Result;
use mando_shared::telegram_format::{escape_html, split_message, status_icon};
use mando_types::ItemStatus;
use serde_json::json;

fn truncate_char_boundary(s: &str, max: usize) -> &str {
    if s.len() <= max {
        s
    } else {
        &s[..s.floor_char_boundary(max)]
    }
}

/// Status display order.
const STATUS_ORDER: &[ItemStatus] = &[
    ItemStatus::New,
    ItemStatus::Clarifying,
    ItemStatus::NeedsClarification,
    ItemStatus::Queued,
    ItemStatus::InProgress,
    ItemStatus::CaptainReviewing,
    ItemStatus::CaptainMerging,
    ItemStatus::AwaitingReview,
    ItemStatus::HandedOff,
    ItemStatus::Rework,
    ItemStatus::Escalated,
    ItemStatus::Errored,
    ItemStatus::Merged,
    ItemStatus::CompletedNoPr,
    ItemStatus::Canceled,
];

fn status_label(s: &ItemStatus) -> &'static str {
    match s {
        ItemStatus::New => "new",
        ItemStatus::Clarifying => "clarifying",
        ItemStatus::NeedsClarification => "needs_clarification",
        ItemStatus::Queued => "queued",
        ItemStatus::InProgress => "in_progress",
        ItemStatus::CaptainReviewing => "captain_reviewing",
        ItemStatus::CaptainMerging => "captain_merging",
        ItemStatus::AwaitingReview => "awaiting_review",
        ItemStatus::HandedOff => "handed_off",
        ItemStatus::Rework => "rework",
        ItemStatus::Escalated => "escalated",
        ItemStatus::Errored => "errored",
        ItemStatus::Merged => "merged",
        ItemStatus::CompletedNoPr => "completed_no_pr",
        ItemStatus::Canceled => "canceled",
    }
}

/// Handle `/status [all]`.
pub async fn handle(bot: &TelegramBot, chat_id: &str, args: &str) -> Result<()> {
    let show_all = args.trim().eq_ignore_ascii_case("all");
    // When show_all, fetch including archived items from the gateway.
    let api_path = if show_all {
        "/api/tasks?include_archived=true"
    } else {
        "/api/tasks"
    };
    let items = match super::load_tasks_with_path(bot.gw(), api_path).await {
        Ok(items) => items,
        Err(e) => {
            let _ = bot
                .send_html(
                    chat_id,
                    &format!(
                        "\u{274c} Failed to load tasks: {}",
                        escape_html(&e.to_string())
                    ),
                )
                .await;
            return Ok(());
        }
    };

    if items.is_empty() {
        bot.send_html(chat_id, "Task list is empty.").await?;
        return Ok(());
    }

    let display: Vec<&mando_types::Task> = items.iter().collect();

    // Summary line
    let mut status_counts: std::collections::HashMap<&str, usize> =
        std::collections::HashMap::new();
    for item in &display {
        let label = status_label(&item.status);
        *status_counts.entry(label).or_default() += 1;
    }
    let summary_parts: Vec<String> = STATUS_ORDER
        .iter()
        .filter_map(|s| {
            let label = status_label(s);
            status_counts.get(label).map(|c| format!("{label}={c}"))
        })
        .collect();

    let mut lines: Vec<String> = Vec::new();
    lines.push(format!("\u{1f4ca} <b>Tasks</b> ({} items)", display.len()));
    lines.push(summary_parts.join(" "));

    let mut merge_buttons: Vec<serde_json::Value> = Vec::new();

    // Group by repo
    let mut by_repo: std::collections::BTreeMap<String, Vec<&mando_types::Task>> =
        std::collections::BTreeMap::new();
    for item in &display {
        let project = item.project.as_deref().unwrap_or("unknown");
        by_repo.entry(project.to_string()).or_default().push(item);
    }

    for (repo, repo_items) in &by_repo {
        lines.push(format!("\n\u{1f4e6} <b>{}</b>", escape_html(repo)));

        // Group by status within repo
        for &status in STATUS_ORDER {
            let status_items: Vec<_> = repo_items.iter().filter(|it| it.status == status).collect();
            if status_items.is_empty() {
                continue;
            }

            let icon = status_icon(status_label(&status));
            lines.push(format!(
                "{} <b>{}</b> ({})",
                icon,
                status_label(&status),
                status_items.len()
            ));

            for item in &status_items {
                // Under a status header, show compact: #id Title (worker | PR #N)
                // Show Linear ID if available, otherwise task ID
                let id_str = item
                    .linear_id
                    .as_deref()
                    .map(|lid| format!("{lid} "))
                    .unwrap_or_else(|| format!("#{} ", item.id));
                let title = escape_html(&item.title);
                let worker = item
                    .worker
                    .as_deref()
                    .filter(|w| !w.is_empty())
                    .map(|w| format!(" | {}", escape_html(w)))
                    .unwrap_or_default();
                let pr_part = item
                    .pr
                    .as_deref()
                    .filter(|p| !p.is_empty())
                    .map(|pr_ref| {
                        let num = pr_ref
                            .rsplit('/')
                            .next()
                            .unwrap_or(pr_ref)
                            .trim_start_matches('#');
                        if pr_ref.starts_with("http") {
                            format!(
                                " | <a href=\"{}\">{}</a>",
                                escape_html(pr_ref),
                                escape_html(&format!("PR #{num}"))
                            )
                        } else {
                            format!(" | PR #{}", escape_html(num))
                        }
                    })
                    .unwrap_or_default();
                lines.push(format!("  \u{2022} {id_str}{title}{worker}{pr_part}"));

                let id = item.id;
                let title_short = truncate_char_boundary(&item.title, 30);
                match status {
                    ItemStatus::AwaitingReview => {
                        if let Some(ref pr) = item.pr {
                            let pr_num = pr.trim_start_matches('#');
                            let ts = truncate_char_boundary(&item.title, 22);
                            merge_buttons.push(json!([{
                                "text": format!("\u{1f500} Merge PR #{pr_num} \u{2014} {ts}"),
                                "callback_data": format!("merge:{id}"),
                            }]));
                        } else {
                            merge_buttons.push(json!([{
                                "text": format!("\u{2705} Accept \u{2014} {title_short}"),
                                "callback_data": format!("accept:{id}"),
                            }]));
                        }
                    }
                    ItemStatus::NeedsClarification => {
                        merge_buttons.push(json!([{
                            "text": format!("\u{1f4ac} Answer \u{2014} {title_short}"),
                            "callback_data": format!("answer:{id}"),
                        }]));
                    }
                    ItemStatus::Errored => {
                        merge_buttons.push(json!([{
                            "text": format!("\u{1f504} Retry \u{2014} {title_short}"),
                            "callback_data": format!("retry:{id}"),
                        }]));
                    }
                    _ => {}
                }
            }
        }
    }

    let text = lines.join("\n");
    let chunks = split_message(&text, 3800);

    for (i, chunk) in chunks.iter().enumerate() {
        let markup = if i == 0 && !merge_buttons.is_empty() {
            Some(json!({"inline_keyboard": merge_buttons}))
        } else {
            None
        };
        bot.api()
            .send_message(chat_id, chunk, Some("HTML"), markup, true)
            .await?;
    }

    Ok(())
}
