//! Telegram HTML formatting utilities.
//!
//! Converts tasks and messages into Telegram-safe HTML.
//!
//! The visible-length truncating markdown renderer lives in
//! `global_infra::tg_markdown` so the captain biz tier can render LLM
//! markdown without depending on the transport tier. Re-exported here so
//! existing call sites keep their imports.

pub use global_infra::tg_markdown::{render_markdown_reply_html, TELEGRAM_TEXT_MAX_LEN};

// ── Core formatting ─────────────────────────────────────────────────

/// Resolve the paused-state display for a task. Returns
/// `(icon, label)` when `paused_until` is in the future, else `None` so
/// the caller falls through to the lifecycle-status icon/label.
///
/// `now_secs` is injected so the caller controls the clock (tests pass a
/// fixed epoch, production passes `SystemTime::now`).
pub fn paused_badge(paused_until: Option<i64>, now_secs: i64) -> Option<(&'static str, String)> {
    let until = paused_until?;
    if until <= now_secs {
        return None;
    }
    // Format as `HH:MM` in UTC since Telegram users may be in any
    // timezone; the credential cooldown is a server-side wall-clock value
    // and showing UTC avoids ambiguous local-time renderings.
    let reset = time::OffsetDateTime::from_unix_timestamp(until)
        .ok()
        .and_then(|dt| {
            time::format_description::parse("[hour]:[minute] UTC")
                .ok()
                .and_then(|fmt| dt.format(&fmt).ok())
        })
        .unwrap_or_else(|| until.to_string());
    Some(("\u{23f8}\u{fe0f}", format!("Paused · resumes {reset}")))
}

/// Return an emoji icon for each item status.
///
/// Accepts both serde-renamed strings (`"needs-clarification"`) and
/// display-label strings (`"needs_clarification"`) for convenience.
pub fn status_icon(status: &str) -> &'static str {
    match status {
        "new" => "\u{1f195}",                                        // NEW
        "clarifying" => "\u{2753}",                                  // question mark
        "needs-clarification" | "needs_clarification" => "\u{2757}", // exclamation mark
        "queued" => "\u{2705}",                                      // check mark
        "in-progress" | "in_progress" => "\u{1f528}",                // hammer
        "captain-reviewing" | "captain_reviewing" => "\u{1f9d0}",    // monocle face
        "captain-merging" | "captain_merging" => "\u{1f680}",        // rocket
        "awaiting-review" | "awaiting_review" => "\u{1f440}",        // eyes
        "rework" => "\u{1f504}",                                     // counterclockwise
        "escalated" => "\u{1f6a8}",                                  // rotating light
        "errored" => "\u{26a0}\u{fe0f}",                             // warning
        "handed-off" | "handed_off" => "\u{1f91d}",                  // handshake
        "merged" => "\u{1f389}",                                     // party popper
        "completed-no-pr" | "completed_no_pr" => "\u{2714}",         // heavy check mark
        "canceled" => "\u{274c}",                                    // cross mark
        _ => "\u{2022}",                                             // bullet
    }
}

/// Escape text for Telegram HTML (`<`, `>`, `&`, `"`).
pub fn escape_html(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            '&' => result.push_str("&amp;"),
            '"' => result.push_str("&quot;"),
            _ => result.push(ch),
        }
    }
    result
}

/// Build a clickable Telegram HTML hyperlink for a PR number.
pub fn pr_html_link(pr_number: i64, github_repo: Option<&str>) -> String {
    let label = escape_html(&format!("PR #{pr_number}"));
    if let Some(repo) = github_repo {
        let url = format!("https://github.com/{repo}/pull/{pr_number}");
        return format!("<a href=\"{}\">{label}</a>", escape_html(&url),);
    }
    label
}

/// Split a long message into chunks at `max_len` boundaries.
///
/// Prefers splitting at newlines; falls back to hard split.
pub fn split_message(text: &str, max_len: usize) -> Vec<String> {
    let max = if max_len == 0 { 3600 } else { max_len };

    if text.len() <= max {
        return vec![text.to_string()];
    }

    let mut chunks = Vec::new();
    let mut remaining = text;

    while !remaining.is_empty() {
        if remaining.len() <= max {
            chunks.push(remaining.to_string());
            break;
        }

        // Clamp to a valid char boundary before slicing (floor_char_boundary
        // is stable since Rust 1.82 — we run 1.92+).
        let byte_limit = remaining.floor_char_boundary(max);
        // Try to split at a newline within the char-safe limit.
        let split_at = remaining[..byte_limit]
            .rfind('\n')
            .map(|pos| pos + 1)
            .unwrap_or(byte_limit);

        chunks.push(remaining[..split_at].to_string());
        remaining = &remaining[split_at..];
    }

    chunks
}
