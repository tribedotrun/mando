//! Task detail card — rich inline view triggered by tapping a task in `/tasks`.

use crate::bot::TelegramBot;
use anyhow::Result;
use mando_shared::telegram_format::{escape_html, status_icon};
use serde_json::{json, Value};

/// Render a task detail card by editing the given message in place.
pub async fn handle_view(bot: &TelegramBot, chat_id: &str, mid: i64, task_id: &str) -> Result<()> {
    let tasks = super::load_tasks_with_path(bot.gw(), "/api/tasks?include_archived=true").await?;
    let task = tasks.iter().find(|t| t.id.to_string() == task_id);

    let Some(task) = task else {
        bot.edit_message(
            chat_id,
            mid,
            &format!("Task #{} not found.", escape_html(task_id)),
        )
        .await?;
        return Ok(());
    };

    let pr_path = format!("/api/tasks/{task_id}/pr-summary");
    let tl_path = format!("/api/tasks/{task_id}/timeline");
    let sess_path = format!("/api/tasks/{task_id}/sessions");
    let (pr_res, timeline_res, sessions_res) = tokio::join!(
        bot.gw().get(&pr_path),
        bot.gw().get(&tl_path),
        bot.gw().get(&sess_path),
    );

    let mut lines = Vec::new();

    // ── Header ──────────────────────────────────────────────────────
    let icon = status_icon(task.status.as_str());
    lines.push(format!(
        "{icon} <b>#{} {}</b>",
        task.id,
        escape_html(super::truncate(&task.title, 60)),
    ));

    let mut meta = vec![format!("<b>{}</b>", escape_html(task.status.as_str()))];
    if let Some(ref p) = task.project {
        meta.push(escape_html(p));
    }
    if let Some(ref w) = task.worker {
        meta.push(escape_html(w));
    }
    lines.push(meta.join(" | "));

    if let Some(ref pr) = task.pr {
        lines.push(mando_shared::helpers::pr_html_link(
            pr,
            task.github_repo.as_deref(),
        ));
    }

    // ── Evidence ────────────────────────────────────────────────────
    render_evidence(&mut lines, &pr_res, task);

    // ── Sessions ────────────────────────────────────────────────────
    if let Ok(ref sess_val) = sessions_res {
        if let Some(sessions) = sess_val["sessions"].as_array() {
            if !sessions.is_empty() {
                let count = sessions.len();
                let total_cost: f64 = sessions.iter().filter_map(|s| s["cost_usd"].as_f64()).sum();
                // Sessions are ordered newest-first (created_at DESC).
                let last_status = escape_html(sessions[0]["status"].as_str().unwrap_or("?"));
                lines.push(String::new());
                lines.push(format!(
                    "<b>Sessions</b>: {count} | ${total_cost:.2} | last: {last_status}",
                ));
            }
        }
    }

    // ── Timeline (last 5) ───────────────────────────────────────────
    if let Ok(ref tl_val) = timeline_res {
        if let Some(events) = tl_val["events"].as_array() {
            if !events.is_empty() {
                lines.push(String::new());
                lines.push("<b>Recent Activity</b>".to_string());
                let last5: Vec<_> = events.iter().rev().take(5).collect();
                for event in last5.into_iter().rev() {
                    let kind = event["event_type"].as_str().unwrap_or("event");
                    let ts = event["timestamp"].as_str().unwrap_or("");
                    let detail = event["summary"].as_str().unwrap_or("");
                    lines.push(format!(
                        "<code>{}</code> {} {}",
                        escape_html(super::truncate(ts, 16)),
                        timeline_icon(kind),
                        escape_html(super::truncate(detail, 60)),
                    ));
                }
            }
        }
    }

    // ── Keyboard ────────────────────────────────────────────────────
    let has_pr = task.pr.as_deref().is_some_and(|p| !p.is_empty());
    let mut buttons =
        crate::commands::action::action_buttons(&task.id.to_string(), task.status, has_pr);
    buttons.push(vec![
        json!({
            "text": "\u{1f4c5} Full Timeline",
            "callback_data": format!("dtl:tl:{}", task.id),
        }),
        json!({
            "text": "\u{2b05}\u{fe0f} Back",
            "callback_data": "dtl:back",
        }),
    ]);

    let text = lines.join("\n");
    let text = truncate_for_telegram(&text, 3800);

    bot.edit_message_with_markup(
        chat_id,
        mid,
        &text,
        Some(json!({"inline_keyboard": buttons})),
    )
    .await?;

    Ok(())
}

// ── Evidence rendering ──────────────────────────────────────────────

fn render_evidence(
    lines: &mut Vec<String>,
    pr_res: &Result<Value, anyhow::Error>,
    task: &mando_types::Task,
) {
    lines.push(String::new());

    if let Ok(ref pr_val) = pr_res {
        let summary = pr_val["summary"].as_str().unwrap_or("");
        if !summary.is_empty() {
            let evidence = extract_evidence_summary(summary);
            if !evidence.is_empty() {
                lines.push("<b>Evidence</b>".to_string());
                lines.push(evidence);
                return;
            }
        }
    }

    if let Some(ref report) = task.escalation_report {
        lines.push("<b>Escalation Report</b>".to_string());
        lines.push(escape_html(super::truncate(report, 300)));
    } else {
        lines.push("<i>No evidence yet</i>".to_string());
    }
}

fn extract_evidence_summary(body: &str) -> String {
    let mut parts = Vec::new();
    let mut in_evidence = false;
    let mut evidence_level = 0usize;
    let mut has_code = false;
    let mut has_video = false;

    for line in body.lines() {
        let trimmed = line.trim();
        let level = trimmed.chars().take_while(|c| *c == '#').count();

        if level > 0 {
            let heading = trimmed[level..].trim().to_ascii_lowercase();
            if !heading.is_empty() {
                if is_evidence_heading(&heading) {
                    in_evidence = true;
                    evidence_level = level;
                    continue;
                } else if in_evidence && level <= evidence_level {
                    in_evidence = false;
                }
            }
        }

        if !in_evidence {
            continue;
        }

        if let Some(url) = extract_md_image_url(trimmed) {
            parts.push(format!(
                "\u{1f5bc} <a href=\"{}\">Screenshot</a>",
                escape_html(url),
            ));
        } else if let Some(url) = extract_html_img_src(trimmed) {
            parts.push(format!(
                "\u{1f5bc} <a href=\"{}\">Screenshot</a>",
                escape_html(url),
            ));
        } else if !has_video
            && (trimmed.contains(".mp4") || trimmed.contains(".mov") || trimmed.contains(".webm"))
        {
            has_video = true;
            parts.push("\u{1f3ac} Video evidence".to_string());
        } else if !has_code && trimmed.starts_with("```") {
            has_code = true;
            parts.push("\u{1f4dd} Code evidence".to_string());
        }
    }

    parts.truncate(5);
    parts.join("\n")
}

fn is_evidence_heading(heading: &str) -> bool {
    matches!(
        heading,
        "after" | "evidence" | "visual evidence" | "before / after" | "before/after"
    )
}

fn extract_md_image_url(line: &str) -> Option<&str> {
    let start = line.find("![")?;
    let rest = &line[start + 2..];
    let close = rest.find(']')?;
    let after = &rest[close + 1..];
    if !after.starts_with('(') {
        return None;
    }
    let end = after[1..].find(')')?;
    Some(&after[1..1 + end])
}

fn extract_html_img_src(line: &str) -> Option<&str> {
    let lower = line.to_ascii_lowercase();
    // Find ` src="` (space-prefixed) to avoid matching `data-src=""`
    let idx = lower.find(" src=\"")?;
    let before = &lower[..idx];
    if !before.contains("<img") {
        return None;
    }
    let idx = idx + 1; // skip the leading space
    let rest = &line[idx + 5..];
    let end = rest.find('"')?;
    Some(&rest[..end])
}

fn timeline_icon(kind: &str) -> &'static str {
    match kind {
        "created" => "\u{2795}",
        "clarify_started" | "clarify_question" => "\u{2753}",
        "clarify_resolved" | "worker_completed" => "\u{2705}",
        "human_answered" => "\u{1f4ac}",
        "worker_spawned" => "\u{1f680}",
        "worker_nudged" => "\u{1f4a5}",
        "session_resumed" | "human_reopen" | "rework_requested" | "status_changed" => "\u{1f504}",
        "captain_review_started" => "\u{1f9d0}",
        "captain_review_verdict" => "\u{2696}\u{fe0f}",
        "awaiting_review" => "\u{1f440}",
        "rebase_triggered" => "\u{1f500}",
        "merged" => "\u{1f389}",
        "escalated" => "\u{1f6a8}",
        "errored" => "\u{26a0}\u{fe0f}",
        "canceled" => "\u{274c}",
        "handed_off" => "\u{1f91d}",
        _ => "\u{2022}",
    }
}

fn truncate_for_telegram(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    // Truncate at the last newline before max so we never split mid-HTML-tag
    // (each line in the detail card is a self-contained HTML element).
    let char_boundary = text.floor_char_boundary(max.saturating_sub(4));
    let boundary = text[..char_boundary].rfind('\n').unwrap_or(char_boundary);
    format!("{}\n...", &text[..boundary])
}
