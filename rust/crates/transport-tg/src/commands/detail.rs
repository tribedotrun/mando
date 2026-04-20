//! Task detail card — rich inline view triggered by tapping a task in `/tasks`.

use crate::bot::TelegramBot;
use crate::telegram_format::{escape_html, status_icon};
use anyhow::Result;
use tracing::warn;

/// Max number of inline photos sent alongside the detail card.
const MAX_INLINE_PHOTOS: usize = 3;

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
        bot.gw().get_typed::<api_types::PrSummaryResponse>(&pr_path),
        bot.gw().get_typed::<api_types::TimelineResponse>(&tl_path),
        bot.gw()
            .get_typed::<api_types::ItemSessionsResponse>(&sess_path),
    );

    let mut lines = Vec::new();

    // -- Header --
    let icon = status_icon(task.status().as_str());
    lines.push(format!(
        "{icon} <b>#{} {}</b>",
        task.id,
        escape_html(super::truncate(&task.title, 60)),
    ));

    // -- Meta line: status | project | worker --
    let mut meta = vec![format!("<b>{}</b>", escape_html(task.status().as_str()))];
    if !task.project.is_empty() {
        meta.push(escape_html(&task.project));
    }
    if let Some(ref w) = task.worker {
        meta.push(escape_html(w));
    }
    lines.push(meta.join(" | "));

    // -- Secondary meta: created_at | branch --
    let mut meta2 = Vec::new();
    if let Some(ref created) = task.created_at {
        meta2.push(format!("\u{1f4c5} {}", format_date(created)));
    }
    if let Some(ref branch) = task.branch {
        if !branch.is_empty() {
            meta2.push(format!(
                "\u{1f33f} {}",
                escape_html(super::truncate(branch, 30))
            ));
        }
    }
    if !meta2.is_empty() {
        lines.push(meta2.join(" | "));
    }

    // -- PR link --
    if let Some(pr_num) = task.pr_number {
        lines.push(crate::telegram_format::pr_html_link(
            pr_num,
            task.github_repo.as_deref(),
        ));
    }

    // -- Original prompt / context --
    let prompt_text = task
        .original_prompt
        .as_deref()
        .or(task.context.as_deref())
        .unwrap_or("");
    if !prompt_text.is_empty() {
        lines.push(String::new());
        lines.push(format!(
            "<i>{}</i>",
            escape_html(super::truncate(prompt_text, 200)),
        ));
    }

    // -- Evidence --
    let evidence = collect_evidence(pr_res.as_ref().ok(), task);
    let evidence_image_urls = match &evidence {
        EvidenceKind::PrEvidence { text, image_urls } => {
            lines.push(String::new());
            lines.push("<b>Evidence</b>".to_string());
            lines.push(text.clone());
            image_urls.clone()
        }
        EvidenceKind::Escalation(report) => {
            lines.push(String::new());
            lines.push("<b>Escalation Report</b>".to_string());
            lines.push(report.clone());
            Vec::new()
        }
        EvidenceKind::None => {
            lines.push(String::new());
            lines.push("<i>No evidence yet</i>".to_string());
            Vec::new()
        }
    };

    // -- Sessions: count | duration | last status --
    if let Ok(ref sess_resp) = sessions_res {
        let sessions = &sess_resp.sessions;
        if !sessions.is_empty() {
            let count = sessions.len();
            let total_ms: u64 = sessions
                .iter()
                .filter_map(|s| s.duration_ms.map(|ms| ms as u64))
                .sum();
            let last_status_str = serde_json::to_value(sessions[0].status)
                .ok()
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| "?".to_string());
            let last_status = escape_html(&last_status_str);
            lines.push(String::new());
            lines.push(format!(
                "<b>Sessions</b>: {count} | {} | last: {last_status}",
                format_duration_ms(total_ms),
            ));
        }
    }

    // -- Timeline (last 5) --
    if let Ok(ref tl_resp) = timeline_res {
        let events = &tl_resp.events;
        if !events.is_empty() {
            lines.push(String::new());
            lines.push("<b>Recent Activity</b>".to_string());
            let last5: Vec<_> = events.iter().rev().take(5).collect();
            for event in last5.into_iter().rev() {
                let kind = event.data.event_type_str();
                let ts = event.timestamp.as_str();
                let detail = event.summary.as_str();
                lines.push(format!(
                    "<code>{}</code> {} {}",
                    escape_html(super::truncate(ts, 16)),
                    timeline_icon(kind),
                    escape_html(super::truncate(detail, 60)),
                ));
            }
        }
    }

    // -- Keyboard --
    let has_pr = task.pr_number.is_some();
    let mut buttons =
        crate::commands::action::action_buttons(&task.id.to_string(), task.status(), has_pr);
    buttons.push(vec![
        api_types::InlineKeyboardButton {
            text: "\u{1f4c5} Full Timeline".into(),
            callback_data: Some(format!("dtl:tl:{}", task.id)),
            url: None,
        },
        api_types::InlineKeyboardButton {
            text: "\u{2b05}\u{fe0f} Back".into(),
            callback_data: Some("dtl:back".into()),
            url: None,
        },
    ]);

    let text = lines.join("\n");
    let text = truncate_for_telegram(&text, 3800);

    bot.edit_message_with_markup(
        chat_id,
        mid,
        &text,
        Some(api_types::TelegramReplyMarkup::InlineKeyboard { rows: buttons }),
    )
    .await?;

    // -- Inline photos (sent as separate messages after the card) --
    send_inline_photos(bot, chat_id, &evidence_image_urls, task).await;

    Ok(())
}

// -- Evidence data collection --

enum EvidenceKind {
    /// PR summary evidence with optional inline image URLs.
    PrEvidence {
        text: String,
        image_urls: Vec<String>,
    },
    /// Escalation report (text only, no images).
    Escalation(String),
    /// Nothing found.
    None,
}

fn collect_evidence(
    pr_res: Option<&api_types::PrSummaryResponse>,
    task: &captain::Task,
) -> EvidenceKind {
    if let Some(pr) = pr_res {
        let summary = pr.summary.as_deref().unwrap_or("");
        if !summary.is_empty() {
            let data = extract_evidence_data(summary);
            if !data.0.is_empty() {
                return EvidenceKind::PrEvidence {
                    text: data.0,
                    image_urls: data.1,
                };
            }
        }
    }

    if let Some(ref report) = task.escalation_report {
        return EvidenceKind::Escalation(escape_html(super::truncate(report, 300)));
    }

    EvidenceKind::None
}

/// Returns (formatted_text, image_urls). Both are truncated to the same cap.
fn extract_evidence_data(body: &str) -> (String, Vec<String>) {
    let mut text_parts = Vec::new();
    let mut image_urls = Vec::new();
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
            text_parts.push(format!(
                "\u{1f5bc} <a href=\"{}\">Screenshot</a>",
                escape_html(url),
            ));
            image_urls.push(url.to_string());
        } else if let Some(url) = extract_html_img_src(trimmed) {
            text_parts.push(format!(
                "\u{1f5bc} <a href=\"{}\">Screenshot</a>",
                escape_html(url),
            ));
            image_urls.push(url.to_string());
        } else if !has_video
            && (trimmed.contains(".mp4") || trimmed.contains(".mov") || trimmed.contains(".webm"))
        {
            has_video = true;
            text_parts.push("\u{1f3ac} Video evidence".to_string());
        } else if !has_code && trimmed.starts_with("```") {
            has_code = true;
            text_parts.push("\u{1f4dd} Code evidence".to_string());
        }
    }

    text_parts.truncate(5);
    image_urls.truncate(MAX_INLINE_PHOTOS);
    (text_parts.join("\n"), image_urls)
}

// -- Inline photo sending --

async fn send_inline_photos(
    bot: &TelegramBot,
    chat_id: &str,
    evidence_urls: &[String],
    task: &captain::Task,
) {
    let mut sent = 0usize;

    // 1. Evidence screenshots (public GCS URLs)
    for url in evidence_urls {
        if sent >= MAX_INLINE_PHOTOS {
            break;
        }
        if let Err(e) = bot.send_photo_url(chat_id, url, None).await {
            warn!(
                module = "transport-tg",
                "Failed to send evidence photo: {e}"
            );
            continue;
        }
        sent += 1;
    }

    // 2. Task images (local files fetched from gateway)
    if sent < MAX_INLINE_PHOTOS {
        if let Some(ref images_str) = task.images {
            let filenames: Vec<&str> = images_str
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect();
            for filename in filenames {
                if sent >= MAX_INLINE_PHOTOS {
                    break;
                }
                match fetch_image_bytes(bot, filename).await {
                    Ok(bytes) => {
                        if let Err(e) = bot.send_photo_bytes(chat_id, bytes, filename, None).await {
                            warn!(
                                module = "transport-tg",
                                "Failed to send task image {filename}: {e}"
                            );
                            continue;
                        }
                        sent += 1;
                    }
                    Err(e) => {
                        warn!(
                            module = "transport-tg",
                            "Failed to fetch task image {filename}: {e}"
                        );
                    }
                }
            }
        }
    }
}

/// Fetch image bytes from the gateway's `/api/images/{filename}` endpoint.
async fn fetch_image_bytes(bot: &TelegramBot, filename: &str) -> Result<Vec<u8>> {
    let gw = bot.gw();
    let url = format!("{}/api/images/{}", gw.base_url(), filename);
    let mut req = gw.client().get(&url);
    if let Some(token) = gw.token() {
        req = req.header("Authorization", format!("Bearer {token}"));
    }
    let resp = req.send().await?;
    anyhow::ensure!(
        resp.status().is_success(),
        "image fetch returned {}",
        resp.status()
    );
    let bytes = resp.bytes().await?;
    Ok(bytes.to_vec())
}

// -- Formatting helpers --

fn format_date(rfc3339: &str) -> String {
    let date_str = match rfc3339.get(..10) {
        Some(s) if s.len() == 10 => s,
        _ => return escape_html(rfc3339),
    };
    let parts: Vec<&str> = date_str.split('-').collect();
    if parts.len() != 3 {
        return escape_html(date_str);
    }
    let month = match parts[1] {
        "01" => "Jan",
        "02" => "Feb",
        "03" => "Mar",
        "04" => "Apr",
        "05" => "May",
        "06" => "Jun",
        "07" => "Jul",
        "08" => "Aug",
        "09" => "Sep",
        "10" => "Oct",
        "11" => "Nov",
        "12" => "Dec",
        _ => parts[1],
    };
    let day = parts[2].trim_start_matches('0');
    format!("{month} {day}")
}

fn format_duration_ms(ms: u64) -> String {
    let secs = ms / 1000;
    let mins = secs / 60;
    let hours = mins / 60;
    if hours > 0 {
        format!("{}h {}m", hours, mins % 60)
    } else if mins > 0 {
        format!("{}m {}s", mins, secs % 60)
    } else {
        format!("{}s", secs)
    }
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
