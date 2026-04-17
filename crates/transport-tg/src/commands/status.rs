//! `/tasks [all]` — task overview grouped by repo → workflow state, with merge/accept buttons.

use crate::bot::TelegramBot;
use crate::telegram_format::{escape_html, split_message, status_icon};
use anyhow::Result;
use captain::ItemStatus;
use serde_json::json;

/// How long finalized tasks (merged/completed/canceled) remain visible in the
/// default `/tasks` view before being hidden. `tasks all` bypasses this.
const FINALIZED_VISIBLE_HOURS: i64 = 8;

/// Workflow state display order.
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
    ItemStatus::PlanReady,
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
        ItemStatus::PlanReady => "plan_ready",
        ItemStatus::Canceled => "canceled",
    }
}

/// Handle `/tasks [all]`.
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
            if let Err(e) = bot
                .send_html(
                    chat_id,
                    &format!(
                        "\u{274c} Failed to load tasks: {}",
                        escape_html(&e.to_string())
                    ),
                )
                .await
            {
                tracing::warn!(module = "telegram", error = %e, "message send failed");
            }
            return Ok(());
        }
    };

    if items.is_empty() {
        bot.send_html(chat_id, "Task list is empty.").await?;
        return Ok(());
    }

    // In the default view, hide finalized tasks older than the visibility window.
    let display: Vec<&captain::Task> = if show_all {
        items.iter().collect()
    } else {
        let cutoff =
            time::OffsetDateTime::now_utc() - time::Duration::hours(FINALIZED_VISIBLE_HOURS);
        items
            .iter()
            .filter(|t| {
                if !t.status.is_finalized() {
                    return true;
                }
                // Use last_activity_at, fall back to created_at. If neither
                // exists, keep the task visible (shouldn't happen in practice).
                let ts_str = t.last_activity_at.as_deref().or(t.created_at.as_deref());
                let Some(ts_str) = ts_str else {
                    return true;
                };
                time::OffsetDateTime::parse(ts_str, &time::format_description::well_known::Rfc3339)
                    .map(|ts| ts > cutoff)
                    .unwrap_or(true)
            })
            .collect()
    };

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

    let mut action_buttons: Vec<serde_json::Value> = Vec::new();
    let mut view_ids: Vec<i64> = Vec::new();

    // Group by repo
    let mut by_repo: std::collections::BTreeMap<String, Vec<&captain::Task>> =
        std::collections::BTreeMap::new();
    for item in &display {
        let project = if item.project.is_empty() {
            "unknown"
        } else {
            &item.project
        };
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
                let id_str = format!("#{} ", item.id);
                let title = escape_html(&item.title);
                let worker = item
                    .worker
                    .as_deref()
                    .filter(|w| !w.is_empty())
                    .map(|w| format!(" | {}", escape_html(w)))
                    .unwrap_or_default();
                let pr_part = item
                    .pr_number
                    .map(|pr_num| {
                        let link = crate::telegram_format::pr_html_link(
                            pr_num,
                            item.github_repo.as_deref(),
                        );
                        format!(" | {link}")
                    })
                    .unwrap_or_default();
                lines.push(format!("  \u{2022} {id_str}{title}{worker}{pr_part}"));

                let id = item.id;
                let title_short = super::truncate(&item.title, 30);
                match status {
                    ItemStatus::AwaitingReview => {
                        if let Some(pr_num) = item.pr_number {
                            action_buttons.push(json!([{
                                "text": format!("Merge PR #{pr_num}"),
                                "callback_data": format!("merge:{id}"),
                            }]));
                        } else {
                            action_buttons.push(json!([{
                                "text": format!("\u{2705} Accept \u{2014} {title_short}"),
                                "callback_data": format!("accept:{id}"),
                            }]));
                        }
                    }
                    ItemStatus::NeedsClarification => {
                        action_buttons.push(json!([{
                            "text": format!("\u{1f4ac} Answer \u{2014} {title_short}"),
                            "callback_data": format!("answer:{id}"),
                        }]));
                    }
                    ItemStatus::Errored => {
                        action_buttons.push(json!([{
                            "text": format!("\u{1f504} Retry \u{2014} {title_short}"),
                            "callback_data": format!("retry:{id}"),
                        }]));
                    }
                    _ => {}
                }

                view_ids.push(id);
            }
        }
    }

    // Per-task detail buttons — ID only to keep the keyboard compact.
    // 5 per row, capped at 15 to stay within Telegram's 100-button limit.
    const MAX_VIEW_BUTTONS: usize = 15;
    let capped = &view_ids[..view_ids.len().min(MAX_VIEW_BUTTONS)];
    for row in capped.chunks(5) {
        let btns: Vec<serde_json::Value> = row
            .iter()
            .map(|id| {
                json!({
                    "text": format!("#{id}"),
                    "callback_data": format!("view:{id}"),
                })
            })
            .collect();
        action_buttons.push(serde_json::Value::Array(btns));
    }

    let text = lines.join("\n");
    let chunks = split_message(&text, 3800);

    for (i, chunk) in chunks.iter().enumerate() {
        let markup = if i == 0 && !action_buttons.is_empty() {
            Some(json!({"inline_keyboard": action_buttons}))
        } else {
            None
        };
        bot.api()
            .send_message(chat_id, chunk, Some("HTML"), markup, true)
            .await?;
    }

    Ok(())
}
