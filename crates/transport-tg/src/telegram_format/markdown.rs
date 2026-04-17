//! Markdown-to-Telegram-HTML converter.
//!
//! Converts Claude-style markdown into Telegram-safe HTML,
//! handling code blocks, inline formatting, and smart URL labels.

use regex::Regex;
use std::sync::LazyLock;

use super::{convert_md_tables, escape_html};

// ── Regexes ────────────────────────────────────────────────────────

static CODE_BLOCK_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?s)```[\w]*\n?(.*?)\n?```").unwrap());

static INLINE_CODE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"`([^`]+)`").unwrap());

static HEADING_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^#{1,6}\s+(.+)$").unwrap());

static BLOCKQUOTE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^>\s*(.*)$").unwrap());

static MARKDOWN_LINK_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[([^\]]+)\]\((https?://[^)]+)\)").unwrap());

static BARE_URL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)https?://[^\s<>()]+").unwrap());

static BOLD_STAR_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\*\*(.+?)\*\*").unwrap());

static BOLD_UNDER_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"__(.+?)__").unwrap());

static ITALIC_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"_([^_]+)_").unwrap());

static STRIKE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"~~(.+?)~~").unwrap());

static LIST_BULLET_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^[-*]\s+").unwrap());

const TRAILING_URL_PUNCT: &str = ".,;:!?)]";

// ── URL labeling ───────────────────────────────────────────────────

fn shorten(value: &str, max_len: usize) -> String {
    if value.len() <= max_len {
        return value.to_string();
    }
    let boundary = value.floor_char_boundary(max_len - 1);
    format!("{}…", &value[..boundary])
}

fn url_label(url: &str) -> String {
    // Minimal URL parsing via split — avoids adding the `url` crate.
    // We expect `https://host/path/segments...`
    let without_scheme = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
        .unwrap_or(url);

    let (host, path) = match without_scheme.find('/') {
        Some(i) => (&without_scheme[..i], &without_scheme[i + 1..]),
        None => (without_scheme, ""),
    };

    let host = host.strip_prefix("www.").unwrap_or(host);
    let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();

    if host.eq_ignore_ascii_case("github.com") && parts.len() >= 4 {
        let kind = parts[2];
        let ident = parts[3];
        if kind == "pull" && ident.chars().all(|c| c.is_ascii_digit()) {
            return format!("PR #{ident}");
        }
        if kind == "issues" && ident.chars().all(|c| c.is_ascii_digit()) {
            return format!("Issue #{ident}");
        }
        if kind == "commit" {
            let short = &ident[..ident.len().min(7)];
            return format!("Commit {short}");
        }
    }

    if !host.is_empty() && !parts.is_empty() {
        return shorten(&format!("{host}/{}", parts[0]), 36);
    }
    if !host.is_empty() {
        return host.to_string();
    }
    "Link".to_string()
}

/// Return true if the URL match at `start` should be skipped
/// (already inside an href attribute or a markdown link).
fn should_skip_url(text: &str, start: usize) -> bool {
    let prefix = &text[start.saturating_sub(6)..start];
    let lower = prefix.to_lowercase();
    if lower.ends_with("href=\"") || lower.ends_with("href='") {
        return true;
    }
    // Skip URLs that are the target of a markdown link: `](url)`
    if start >= 2 && &text[start - 2..start] == "](" {
        return true;
    }
    false
}

/// Replace bare URLs using a caller-provided formatter.
///
/// Shared core for both `auto_link_urls` (HTML output) and
/// `autolink_markdown_urls` (markdown-link output).
fn replace_bare_urls(text: &str, fmt: fn(&str, &str) -> String) -> String {
    BARE_URL_RE
        .replace_all(text, |caps: &regex::Captures| {
            let start = caps.get(0).unwrap().start();
            if should_skip_url(text, start) {
                return caps[0].to_string();
            }

            let raw = &caps[0];
            let mut url = raw.to_string();
            let mut suffix = String::new();
            while url.ends_with(|c: char| TRAILING_URL_PUNCT.contains(c)) {
                let ch = url.pop().unwrap();
                suffix.insert(0, ch);
            }

            let label = url_label(&url);
            format!("{}{suffix}", fmt(&label, &url))
        })
        .into_owned()
}

/// Detect bare URLs in text and wrap them in `<a>` tags with smart labels.
#[cfg(test)]
fn auto_link_urls(text: &str) -> String {
    replace_bare_urls(text, |label, url| format!("<a href=\"{url}\">{label}</a>"))
}

/// Auto-link bare URLs as markdown links (`[label](url)`).
fn autolink_markdown_urls(text: &str) -> String {
    replace_bare_urls(text, |label, url| format!("[{label}]({url})"))
}

// ── Placeholder save/restore ───────────────────────────────────────

fn save_code_blocks(text: &str) -> (String, Vec<String>) {
    let mut blocks = Vec::new();
    let result = CODE_BLOCK_RE
        .replace_all(text, |caps: &regex::Captures| {
            blocks.push(caps[1].to_string());
            format!("\x00CB{}\x00", blocks.len() - 1)
        })
        .into_owned();
    (result, blocks)
}

fn save_inline_codes(text: &str) -> (String, Vec<String>) {
    let mut codes = Vec::new();
    let result = INLINE_CODE_RE
        .replace_all(text, |caps: &regex::Captures| {
            codes.push(caps[1].to_string());
            format!("\x00IC{}\x00", codes.len() - 1)
        })
        .into_owned();
    (result, codes)
}

/// Apply italic: `_text_` → `<i>text</i>`, but only when `_` is not
/// adjacent to an alphanumeric char (simulates look-around).
fn apply_italic(text: &str) -> String {
    ITALIC_RE
        .replace_all(text, |caps: &regex::Captures| {
            let m = caps.get(0).unwrap();
            let start = m.start();
            let end = m.end();
            let bytes = text.as_bytes();
            // Check char before opening `_`.
            if start > 0 && bytes[start - 1].is_ascii_alphanumeric() {
                return caps[0].to_string();
            }
            // Check char after closing `_`.
            if end < bytes.len() && bytes[end].is_ascii_alphanumeric() {
                return caps[0].to_string();
            }
            format!("<i>{}</i>", &caps[1])
        })
        .into_owned()
}

// ── LLM text normalization ──────────────────────────────────────────

/// Replace literal two-character `\n` sequences with real newlines.
///
/// LLMs in structured-output / JSON mode often emit escaped `\n` inside
/// string values.  After JSON parsing, these survive as the literal
/// characters `\` + `n` rather than a real newline.  This function
/// normalizes them so downstream rendering works correctly.
fn normalize_llm_newlines(text: &str) -> String {
    text.replace("\\n", "\n")
}

// ── Public API ─────────────────────────────────────────────────────

/// Convert markdown text to Telegram-safe HTML.
///
/// Handles code blocks, inline code, bold/italic/strikethrough,
/// headings, blockquotes, lists, markdown links, and bare URLs.
pub fn markdown_to_telegram_html(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }

    // 0) Normalize literal `\n` from LLM structured-output responses.
    let text = normalize_llm_newlines(text);

    // 1) Protect code blocks from further processing.
    let (mut text, code_blocks) = save_code_blocks(&text);

    // 2) Convert markdown tables to row-by-row text.
    text = convert_md_tables(&text);

    // 3) Protect inline code.
    let (mut text, inline_codes) = save_inline_codes(&text);

    // 4) Strip headings and blockquotes.
    text = HEADING_RE.replace_all(&text, "$1").into_owned();
    text = BLOCKQUOTE_RE.replace_all(&text, "$1").into_owned();

    // 5) Auto-link bare URLs (as markdown links).
    text = autolink_markdown_urls(&text);

    // 6) HTML-escape non-code text.
    text = escape_html(&text);

    // 7) Convert markdown links → <a> tags (after escaping so URLs are safe).
    text = MARKDOWN_LINK_RE
        .replace_all(&text, r#"<a href="$2">$1</a>"#)
        .into_owned();

    // 8) Inline formatting.
    text = BOLD_STAR_RE.replace_all(&text, "<b>$1</b>").into_owned();
    text = BOLD_UNDER_RE.replace_all(&text, "<b>$1</b>").into_owned();
    text = apply_italic(&text);
    text = STRIKE_RE.replace_all(&text, "<s>$1</s>").into_owned();
    text = LIST_BULLET_RE.replace_all(&text, "\u{2022} ").into_owned();

    // 9) Restore inline code.
    for (i, code) in inline_codes.iter().enumerate() {
        let placeholder = format!("\x00IC{i}\x00");
        let escaped = escape_html(code);
        text = text.replace(&placeholder, &format!("<code>{escaped}</code>"));
    }

    // 10) Restore code blocks.
    for (i, code) in code_blocks.iter().enumerate() {
        let placeholder = format!("\x00CB{i}\x00");
        let escaped = escape_html(code);
        text = text.replace(&placeholder, &format!("<pre><code>{escaped}</code></pre>"));
    }

    text
}

/// Fallback plain-text renderer that strips markdown formatting.
///
/// URLs are replaced with short labels; all markup is removed.
pub fn markdown_to_telegram_plain_text(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }

    // Normalize literal `\n` from LLM structured-output responses.
    let text = normalize_llm_newlines(text);

    let text = convert_md_tables(&text);
    let (mut text, code_blocks) = save_code_blocks(&text);
    text = autolink_markdown_urls(&text);

    // Strip markdown link syntax → keep label only.
    text = MARKDOWN_LINK_RE.replace_all(&text, "$1").into_owned();

    // Strip headings and blockquotes.
    text = HEADING_RE.replace_all(&text, "$1").into_owned();
    text = BLOCKQUOTE_RE.replace_all(&text, "$1").into_owned();

    // Strip inline formatting.
    text = BOLD_STAR_RE.replace_all(&text, "$1").into_owned();
    text = BOLD_UNDER_RE.replace_all(&text, "$1").into_owned();
    text = STRIKE_RE.replace_all(&text, "$1").into_owned();
    text = INLINE_CODE_RE.replace_all(&text, "$1").into_owned();
    text = LIST_BULLET_RE.replace_all(&text, "\u{2022} ").into_owned();

    // Replace remaining bare URLs with short labels.
    text = BARE_URL_RE
        .replace_all(&text, |caps: &regex::Captures| url_label(&caps[0]))
        .into_owned();

    for (i, code) in code_blocks.iter().enumerate() {
        let placeholder = format!("\x00CB{i}\x00");
        text = text.replace(&placeholder, code);
    }

    text
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── normalize_llm_newlines ──────────────────────────────────────

    #[test]
    fn normalize_llm_newlines_replaces_literal() {
        assert_eq!(normalize_llm_newlines("a\\nb\\nc"), "a\nb\nc");
    }

    #[test]
    fn normalize_llm_newlines_preserves_real_newlines() {
        assert_eq!(normalize_llm_newlines("a\nb\nc"), "a\nb\nc");
    }

    #[test]
    fn markdown_html_normalizes_literal_newlines() {
        let html = markdown_to_telegram_html("**bold**\\n- item one");
        assert!(html.contains("<b>bold</b>"));
        assert!(html.contains("\u{2022} item one"));
        assert!(!html.contains("\\n"));
    }

    // ── url_label ──────────────────────────────────────────────────

    #[test]
    fn url_label_github_pr() {
        assert_eq!(url_label("https://github.com/org/repo/pull/42"), "PR #42");
    }

    #[test]
    fn url_label_github_issue() {
        assert_eq!(
            url_label("https://github.com/org/repo/issues/7"),
            "Issue #7"
        );
    }

    #[test]
    fn url_label_github_commit() {
        let label = url_label("https://github.com/org/repo/commit/abc1234567890");
        assert_eq!(label, "Commit abc1234");
    }

    #[test]
    fn url_label_generic_with_path() {
        let label = url_label("https://example.com/foo/bar");
        assert_eq!(label, "example.com/foo");
    }

    #[test]
    fn url_label_host_only() {
        assert_eq!(url_label("https://example.com"), "example.com");
    }

    #[test]
    fn url_label_strips_www() {
        assert_eq!(url_label("https://www.example.com"), "example.com");
    }

    // ── auto_link_urls ─────────────────────────────────────────────

    #[test]
    fn auto_link_bare_url() {
        let result = auto_link_urls("Check https://github.com/org/repo/pull/5 now");
        assert!(result.contains(r#"<a href="https://github.com/org/repo/pull/5">PR #5</a>"#));
    }

    #[test]
    fn auto_link_preserves_trailing_punct() {
        let result = auto_link_urls("See https://example.com/foo.");
        assert!(result.ends_with("</a>."));
    }

    #[test]
    fn auto_link_skips_href() {
        let input = r#"<a href="https://example.com">link</a>"#;
        assert_eq!(auto_link_urls(input), input);
    }

    // ── markdown_to_telegram_html ──────────────────────────────────

    #[test]
    fn html_empty_input() {
        assert_eq!(markdown_to_telegram_html(""), "");
    }

    #[test]
    fn html_code_blocks() {
        let md = "before\n```rust\nfn main() {}\n```\nafter";
        let html = markdown_to_telegram_html(md);
        assert!(html.contains("<pre><code>fn main() {}</code></pre>"));
        assert!(html.contains("before"));
        assert!(html.contains("after"));
    }

    #[test]
    fn html_inline_code() {
        let html = markdown_to_telegram_html("Use `foo()` here");
        assert!(html.contains("<code>foo()</code>"));
    }

    #[test]
    fn html_bold_and_italic() {
        let html = markdown_to_telegram_html("**bold** and _italic_");
        assert!(html.contains("<b>bold</b>"));
        assert!(html.contains("<i>italic</i>"));
    }

    #[test]
    fn html_strikethrough() {
        let html = markdown_to_telegram_html("~~removed~~");
        assert!(html.contains("<s>removed</s>"));
    }

    #[test]
    fn html_escapes_entities() {
        let html = markdown_to_telegram_html("x < y & z > w");
        assert!(html.contains("&lt;"));
        assert!(html.contains("&amp;"));
        assert!(html.contains("&gt;"));
    }

    #[test]
    fn html_code_blocks_not_double_escaped() {
        let md = "```\nx < y\n```";
        let html = markdown_to_telegram_html(md);
        // Should be escaped once, not double-escaped.
        assert!(html.contains("x &lt; y"));
        assert!(!html.contains("&amp;lt;"));
    }

    #[test]
    fn html_markdown_link() {
        let md = "See [docs](https://example.com/docs)";
        let html = markdown_to_telegram_html(md);
        assert!(html.contains(r#"<a href="https://example.com/docs">docs</a>"#));
    }

    #[test]
    fn html_bare_url_github_pr() {
        let md = "Fixed in https://github.com/org/repo/pull/99";
        let html = markdown_to_telegram_html(md);
        assert!(html.contains("PR #99"));
        assert!(html.contains("<a href="));
    }

    #[test]
    fn html_heading_stripped() {
        let html = markdown_to_telegram_html("## Summary\nDetails here");
        assert!(!html.contains('#'));
        assert!(html.contains("Summary"));
    }

    #[test]
    fn html_list_bullets() {
        let html = markdown_to_telegram_html("- item one\n* item two");
        assert!(html.contains("\u{2022} item one"));
        assert!(html.contains("\u{2022} item two"));
    }

    #[test]
    fn html_nested_bold_in_list() {
        let html = markdown_to_telegram_html("- **important** item");
        assert!(html.contains("<b>important</b>"));
        assert!(html.contains("\u{2022}"));
    }

    // ── markdown_to_telegram_plain_text ────────────────────────────

    #[test]
    fn plain_empty() {
        assert_eq!(markdown_to_telegram_plain_text(""), "");
    }

    #[test]
    fn plain_strips_bold() {
        let result = markdown_to_telegram_plain_text("**bold** text");
        assert_eq!(result, "bold text");
    }

    #[test]
    fn plain_strips_inline_code() {
        let result = markdown_to_telegram_plain_text("Use `foo` here");
        assert_eq!(result, "Use foo here");
    }

    #[test]
    fn plain_strips_code_fence_syntax() {
        let result = markdown_to_telegram_plain_text("```rust\nlet value = 1;\n```");
        assert_eq!(result, "let value = 1;");
    }

    #[test]
    fn plain_replaces_urls_with_labels() {
        let result = markdown_to_telegram_plain_text("See https://github.com/org/repo/pull/5");
        assert!(result.contains("PR #5"));
        assert!(!result.contains("https://"));
    }

    #[test]
    fn plain_strips_link_syntax() {
        let result = markdown_to_telegram_plain_text("[click](https://example.com)");
        assert_eq!(result, "click");
    }
}
