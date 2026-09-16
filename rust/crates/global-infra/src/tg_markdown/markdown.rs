//! Markdown-to-Telegram-HTML converter.
//!
//! Converts Claude-style markdown into Telegram-safe HTML,
//! handling code blocks, inline formatting, and smart URL labels.

use regex::Regex;
use std::sync::LazyLock;

use super::convert_md_tables;
use crate::html::escape_html;

// ── Regexes ────────────────────────────────────────────────────────

static CODE_BLOCK_RE: LazyLock<Regex> =
    LazyLock::new(|| match Regex::new(r"(?s)```[\w]*\n?(.*?)\n?```") {
        Ok(re) => re,
        Err(e) => crate::unrecoverable!("regex compile failed", e),
    });

static INLINE_CODE_RE: LazyLock<Regex> = LazyLock::new(|| match Regex::new(r"`([^`]+)`") {
    Ok(re) => re,
    Err(e) => crate::unrecoverable!("regex compile failed", e),
});

static HEADING_RE: LazyLock<Regex> = LazyLock::new(|| match Regex::new(r"(?m)^#{1,6}\s+(.+)$") {
    Ok(re) => re,
    Err(e) => crate::unrecoverable!("regex compile failed", e),
});

static BLOCKQUOTE_RE: LazyLock<Regex> = LazyLock::new(|| match Regex::new(r"(?m)^>\s*(.*)$") {
    Ok(re) => re,
    Err(e) => crate::unrecoverable!("regex compile failed", e),
});

static MARKDOWN_LINK_RE: LazyLock<Regex> =
    LazyLock::new(|| match Regex::new(r"\[([^\]]+)\]\((https?://[^)]+)\)") {
        Ok(re) => re,
        Err(e) => crate::unrecoverable!("MARKDOWN_LINK_RE compile failed", e),
    });

static BARE_URL_RE: LazyLock<Regex> =
    LazyLock::new(|| match Regex::new(r"(?i)https?://[^\s<>()]+") {
        Ok(re) => re,
        Err(e) => crate::unrecoverable!("regex compile failed", e),
    });

static BOLD_STAR_RE: LazyLock<Regex> = LazyLock::new(|| match Regex::new(r"\*\*(.+?)\*\*") {
    Ok(re) => re,
    Err(e) => crate::unrecoverable!("regex compile failed", e),
});

static BOLD_UNDER_RE: LazyLock<Regex> = LazyLock::new(|| match Regex::new(r"__(.+?)__") {
    Ok(re) => re,
    Err(e) => crate::unrecoverable!("regex compile failed", e),
});

static ITALIC_RE: LazyLock<Regex> = LazyLock::new(|| match Regex::new(r"_([^_]+)_") {
    Ok(re) => re,
    Err(e) => crate::unrecoverable!("regex compile failed", e),
});

static STRIKE_RE: LazyLock<Regex> = LazyLock::new(|| match Regex::new(r"~~(.+?)~~") {
    Ok(re) => re,
    Err(e) => crate::unrecoverable!("regex compile failed", e),
});

static LIST_BULLET_RE: LazyLock<Regex> = LazyLock::new(|| match Regex::new(r"(?m)^[-*]\s+") {
    Ok(re) => re,
    Err(e) => crate::unrecoverable!("regex compile failed", e),
});

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
            // `caps.get(0)` is always Some on a successful regex match.
            let Some(m) = caps.get(0) else {
                return String::new();
            };
            let start = m.start();
            if should_skip_url(text, start) {
                return caps[0].to_string();
            }

            let raw = &caps[0];
            let mut url = raw.to_string();
            let mut suffix = String::new();
            while url.ends_with(|c: char| TRAILING_URL_PUNCT.contains(c)) {
                let Some(ch) = url.pop() else { break };
                suffix.insert(0, ch);
            }

            let label = url_label(&url);
            format!("{}{suffix}", fmt(&label, &url))
        })
        .into_owned()
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
            let Some(m) = caps.get(0) else {
                return String::new();
            };
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
