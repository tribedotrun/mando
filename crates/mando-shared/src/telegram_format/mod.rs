//! Telegram HTML formatting utilities.
//!
//! Converts tasks and messages into Telegram-safe HTML.

mod markdown;
mod render;

pub use markdown::{markdown_to_telegram_html, markdown_to_telegram_plain_text};
pub use render::{render_markdown_reply_html, TELEGRAM_TEXT_MAX_LEN};

use regex::Regex;
use std::sync::LazyLock;

// Re-export table conversion from dedicated module.
pub use crate::telegram_tables::convert_md_tables;

// ── PR / issue linkification ────────────────────────────────────────

/// Matches `PR #123` or `PR#123` (case-insensitive, requires PR prefix).
static PR_REF_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)\bPR\s*#(\d+)").unwrap());

/// Extract `owner/repo` from a git remote URL (SSH or HTTPS).
///
/// Delegates to `mando_config::parse_github_slug` — single implementation.
pub fn repo_slug_from_remote(remote_url: &str) -> Option<String> {
    mando_config::parse_github_slug(remote_url)
}

/// Scan `text` for PR references (`#123`, `PR #123`) and replace each with
/// a clickable Telegram hyperlink pointing at the GitHub PR.
pub fn linkify_pr_refs(text: &str, repo_slug: &str) -> String {
    let repo_name = repo_slug.rsplit('/').next().unwrap_or(repo_slug);
    PR_REF_RE
        .replace_all(text, |caps: &regex::Captures| {
            let num = &caps[1];
            let url = format!("https://github.com/{repo_slug}/pull/{num}");
            format!(
                "<a href=\"{url}\">{repo} PR #{num}</a>",
                repo = escape_html(repo_name),
            )
        })
        .into_owned()
}

// ── Core formatting ─────────────────────────────────────────────────

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

/// Generate a Telegram HTML hyperlink: `<a href="url">label</a>`.
pub fn hyperlink(label: &str, url: &str) -> String {
    format!(
        "<a href=\"{}\">{}</a>",
        escape_html(url),
        escape_html(label),
    )
}

#[cfg(test)]
use mando_types::Task;

/// Format a task as a single Telegram line.
///
/// Includes Worker and PR columns.
/// Example: `[hammer] #42 Fix the bug | mando-w-12 | PR #396`
///
/// `github_repo`: optional GitHub slug (e.g. "owner/repo") for constructing
/// PR links when the PR reference is not a full URL. Pass `None` if unknown.
#[cfg(test)]
fn format_item_line(item: &Task, include_repo: bool) -> String {
    format_item_line_with_repo(item, include_repo, None)
}

/// Format a task with an explicit GitHub repo for PR link construction.
#[cfg(test)]
fn format_item_line_with_repo(
    item: &Task,
    include_repo: bool,
    github_repo: Option<&str>,
) -> String {
    let status_str = item.status.as_str();
    let icon = status_icon(status_str);
    let id_display = format!("#{} ", item.id);
    let title = escape_html(&item.title);

    let repo_suffix = if include_repo {
        item.project
            .as_deref()
            .map(|r| format!(" ({})", escape_html(r)))
            .unwrap_or_default()
    } else {
        String::new()
    };

    let worker_part = item
        .worker
        .as_deref()
        .filter(|w| !w.is_empty())
        .map(|w| format!(" | {}", escape_html(w)))
        .unwrap_or_default();

    let pr_link = item
        .pr
        .as_deref()
        .filter(|p| !p.is_empty())
        .map(|pr_ref| {
            // Extract PR number — handle both "#123" and full URL ".../pull/123"
            let num = pr_ref
                .rsplit('/')
                .next()
                .unwrap_or(pr_ref)
                .trim_start_matches('#');
            let label = format!("PR #{num}");
            // Build GitHub URL if we have the repo slug
            let url = if pr_ref.starts_with("http") {
                pr_ref.to_string()
            } else {
                github_repo
                    .map(|repo| format!("https://github.com/{repo}/pull/{num}"))
                    .unwrap_or_default()
            };
            if url.is_empty() {
                format!(" | {label}")
            } else {
                format!(" | {}", hyperlink(&label, &url))
            }
        })
        .unwrap_or_default();

    format!("{icon} {id_display}{title}{repo_suffix}{worker_part}{pr_link}")
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

#[cfg(test)]
mod tests {
    use super::*;
    use mando_types::ItemStatus;

    #[test]
    fn escape_html_special_chars() {
        assert_eq!(escape_html("<b>bold</b>"), "&lt;b&gt;bold&lt;/b&gt;");
        assert_eq!(escape_html("A & B"), "A &amp; B");
        assert_eq!(escape_html("a=\"1\""), "a=&quot;1&quot;");
    }

    #[test]
    fn hyperlink_produces_correct_html() {
        let link = hyperlink("Click me", "https://example.com");
        assert_eq!(link, "<a href=\"https://example.com\">Click me</a>");
    }

    #[test]
    fn hyperlink_escapes_label() {
        let link = hyperlink("A<B", "https://example.com");
        assert_eq!(link, "<a href=\"https://example.com\">A&lt;B</a>");
    }

    #[test]
    fn status_icon_known() {
        assert_eq!(status_icon("in-progress"), "\u{1f528}");
        assert_eq!(status_icon("in_progress"), "\u{1f528}");
        assert_eq!(status_icon("merged"), "\u{1f389}");
        assert_eq!(status_icon("captain-reviewing"), "\u{1f9d0}");
        assert_eq!(status_icon("captain_reviewing"), "\u{1f9d0}");
        assert_eq!(status_icon("needs-clarification"), "\u{2757}");
        assert_eq!(status_icon("escalated"), "\u{1f6a8}");
        assert_eq!(status_icon("errored"), "\u{26a0}\u{fe0f}");
    }

    #[test]
    fn status_icon_unknown() {
        assert_eq!(status_icon("unknown-status"), "\u{2022}");
    }

    #[test]
    fn format_item_line_basic() {
        let mut item = Task::new("Fix the bug");
        item.status = ItemStatus::InProgress;
        item.id = 42;
        let line = format_item_line(&item, false);
        assert!(line.contains("#42"));
        assert!(line.contains("Fix the bug"));
        assert!(line.contains("\u{1f528}"));
    }

    #[test]
    fn format_item_line_with_repo() {
        let mut item = Task::new("Add feature");
        item.status = ItemStatus::Queued;
        item.project = Some("mando".into());
        let line = format_item_line(&item, true);
        assert!(line.contains("(mando)"));
    }

    #[test]
    fn format_item_line_with_worker_and_pr() {
        let mut item = Task::new("Fix auth flow");
        item.status = ItemStatus::InProgress;
        item.id = 12;
        item.worker = Some("mando-w-12".into());
        item.pr = Some("396".into());
        let line = super::format_item_line_with_repo(&item, false, Some("acme/widgets"));
        assert!(line.contains("mando-w-12"), "should include worker name");
        assert!(line.contains("PR #396"), "should include PR number");
        assert!(
            line.contains("| mando-w-12"),
            "worker should be pipe-separated"
        );
        assert!(
            line.contains("| <a href="),
            "PR should be pipe-separated hyperlink"
        );
    }

    #[test]
    fn format_item_line_without_worker_or_pr() {
        let mut item = Task::new("Refactor config");
        item.status = ItemStatus::Queued;
        item.id = 15;
        let line = format_item_line(&item, false);
        assert!(
            !line.contains(" | "),
            "should not have pipe separators when no worker/PR"
        );
    }

    #[test]
    fn split_message_short() {
        let text = "short message";
        let parts = split_message(text, 100);
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0], "short message");
    }

    #[test]
    fn split_message_long() {
        let text = "line one\nline two\nline three\nline four";
        let parts = split_message(text, 20);
        assert!(parts.len() >= 2);
        for part in &parts {
            assert!(part.len() <= 20);
        }
    }

    #[test]
    fn split_message_default_max() {
        let short = "hello";
        let parts = split_message(short, 0);
        assert_eq!(parts.len(), 1);
    }

    #[test]
    fn split_message_emoji_boundary() {
        let text = "abc\u{1f528}defghijklmnop";
        let parts = split_message(text, 5);
        assert!(!parts.is_empty());
        let rejoined: String = parts.join("");
        assert_eq!(rejoined, text);
    }

    // ── PR / issue linkification ────────────────────────────────────

    #[test]
    fn repo_slug_from_ssh_remote() {
        assert_eq!(
            repo_slug_from_remote("git@github.com:acme/widgets.git"),
            Some("acme/widgets".into()),
        );
    }

    #[test]
    fn repo_slug_from_https_remote() {
        assert_eq!(
            repo_slug_from_remote("https://github.com/acme/widgets.git"),
            Some("acme/widgets".into()),
        );
        assert_eq!(
            repo_slug_from_remote("https://github.com/acme/widgets"),
            Some("acme/widgets".into()),
        );
    }

    #[test]
    fn repo_slug_from_invalid_remote() {
        assert_eq!(repo_slug_from_remote("not-a-url"), None);
    }

    #[test]
    fn linkify_pr_refs_replaces_all() {
        let text = "Fixed PR #10 and PR #20";
        let result = linkify_pr_refs(text, "acme/widgets");
        assert!(result.contains("widgets PR #10"));
        assert!(result.contains("widgets PR #20"));
        assert!(result.contains("https://github.com/acme/widgets/pull/10"));
    }

    #[test]
    fn linkify_pr_refs_ignores_bare_hash() {
        let text = "See item #42 for details";
        let result = linkify_pr_refs(text, "acme/widgets");
        assert_eq!(result, text, "bare #N should not be linkified");
    }

    #[test]
    fn linkify_pr_refs_with_pr_prefix() {
        let text = "See PR #446 for details";
        let result = linkify_pr_refs(text, "acme/widgets");
        assert!(result.contains("widgets PR #446"));
        assert!(!result.contains("See PR #446"));
    }
}
