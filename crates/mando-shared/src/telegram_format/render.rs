//! Rendering helpers for model-generated Telegram replies.

use super::{markdown_to_telegram_html, markdown_to_telegram_plain_text};

pub const TELEGRAM_TEXT_MAX_LEN: usize = 4096;
const TRUNCATED_SUFFIX_HTML: &str = "\n\n<i>(truncated)</i>";
const TRUNCATED_SUFFIX_VISIBLE: &str = "\n\n(truncated)";

/// Render model markdown into Telegram HTML with visible-length truncation.
pub fn render_markdown_reply_html(markdown: &str, visible_budget: usize) -> String {
    if markdown.is_empty() || visible_budget == 0 {
        return String::new();
    }

    let visible_budget = visible_budget.min(TELEGRAM_TEXT_MAX_LEN);
    let full_plain = markdown_to_telegram_plain_text(markdown);
    if full_plain.len() <= visible_budget {
        return markdown_to_telegram_html(markdown);
    }

    let truncated_budget = visible_budget.saturating_sub(TRUNCATED_SUFFIX_VISIBLE.len());
    match best_truncated_candidate(markdown, truncated_budget) {
        Some(candidate) => {
            let candidate_html = markdown_to_telegram_html(&candidate);
            format!("{candidate_html}{TRUNCATED_SUFFIX_HTML}")
        }
        None => String::new(),
    }
}

fn best_truncated_candidate(markdown: &str, truncated_budget: usize) -> Option<String> {
    if truncated_budget == 0 {
        return None;
    }

    let boundaries = char_boundaries(markdown);
    let mut low = 1usize;
    let mut high = boundaries.len().saturating_sub(1);
    let mut best = None;

    while low <= high {
        let mid = low + (high - low) / 2;
        let candidate = sanitize_truncated_markdown(&markdown[..boundaries[mid]]);
        if candidate.is_empty() {
            low = mid + 1;
            continue;
        }

        let candidate_plain = markdown_to_telegram_plain_text(&candidate);
        if candidate_plain.len() <= truncated_budget {
            best = Some(candidate);
            low = mid + 1;
        } else {
            high = mid - 1;
        }
    }

    best
}

fn char_boundaries(text: &str) -> Vec<usize> {
    text.char_indices()
        .map(|(idx, _)| idx)
        .chain(std::iter::once(text.len()))
        .collect()
}

fn sanitize_truncated_markdown(text: &str) -> String {
    let mut candidate = text.trim_end().to_string();
    loop {
        let next = close_unterminated_fenced_block(&candidate);
        let next = strip_incomplete_markdown_suffix(&next);
        if next == candidate {
            return candidate;
        }
        candidate = next;
    }
}

fn close_unterminated_fenced_block(text: &str) -> String {
    if text.matches("```").count().is_multiple_of(2) {
        return text.to_string();
    }

    let mut closed = text.trim_end().to_string();
    if !closed.ends_with('\n') {
        closed.push('\n');
    }
    closed.push_str("```");
    closed
}

fn strip_incomplete_markdown_suffix(text: &str) -> String {
    let mut candidate = text.trim_end().to_string();

    if let Some(cutoff) = last_incomplete_marker_start(&candidate) {
        candidate.truncate(cutoff);
        candidate = candidate.trim_end().to_string();
    }

    let trimmed = if candidate.ends_with("```") {
        candidate.trim_end_matches(['*', '_', '~', '['])
    } else {
        candidate.trim_end_matches(['*', '_', '~', '[', '`'])
    };
    if trimmed.len() < candidate.len() {
        trimmed.trim_end().to_string()
    } else {
        candidate
    }
}

fn last_incomplete_marker_start(text: &str) -> Option<usize> {
    let mut byte = 0;
    let mut in_fenced_block = false;
    let mut inline_code: Option<usize> = None;
    let mut bold_star = Vec::new();
    let mut bold_under = Vec::new();
    let mut strike = Vec::new();
    let mut open_bracket: Option<usize> = None;
    let mut pending_link: Option<usize> = None;

    while byte < text.len() {
        let rest = &text[byte..];
        if rest.starts_with("```") {
            in_fenced_block = !in_fenced_block;
            byte += 3;
            continue;
        }

        if in_fenced_block {
            byte += rest.chars().next().unwrap().len_utf8();
            continue;
        }

        if rest.starts_with("**") {
            toggle_stack(&mut bold_star, byte);
            byte += 2;
            continue;
        }

        if rest.starts_with("__") {
            toggle_stack(&mut bold_under, byte);
            byte += 2;
            continue;
        }

        if rest.starts_with("~~") {
            toggle_stack(&mut strike, byte);
            byte += 2;
            continue;
        }

        if rest.starts_with('`') {
            inline_code = if inline_code.is_some() {
                None
            } else {
                Some(byte)
            };
            byte += 1;
            continue;
        }

        if rest.starts_with('[') {
            open_bracket = Some(byte);
            byte += 1;
            continue;
        }

        if rest.starts_with("](") {
            if let Some(open) = open_bracket.take() {
                pending_link = Some(open);
            }
            byte += 2;
            continue;
        }

        if rest.starts_with(')') {
            pending_link = None;
            byte += 1;
            continue;
        }

        byte += rest.chars().next().unwrap().len_utf8();
    }

    [
        inline_code,
        bold_star.last().copied(),
        bold_under.last().copied(),
        strike.last().copied(),
        open_bracket,
        pending_link,
    ]
    .into_iter()
    .flatten()
    .max()
}

fn toggle_stack(stack: &mut Vec<usize>, byte: usize) {
    if stack.pop().is_none() {
        stack.push(byte);
    }
}

#[cfg(test)]
mod tests {
    use super::render_markdown_reply_html;
    use super::{markdown_to_telegram_html, markdown_to_telegram_plain_text};

    #[test]
    fn render_reply_preserves_markdown_formatting() {
        let html = render_markdown_reply_html("**bold** with `code`", 200);
        assert!(html.contains("<b>bold</b>"));
        assert!(html.contains("<code>code</code>"));
        assert!(!html.contains("**"));
        assert!(!html.contains("`code`"));
    }

    #[test]
    fn render_reply_truncates_on_visible_budget() {
        let html = render_markdown_reply_html("1234567890 **abcdefghij**", 16);
        assert!(html.contains("<i>(truncated)</i>"));
        assert!(!html.contains("abcdefghij"));
    }

    #[test]
    fn render_reply_strips_dangling_markdown_when_truncated() {
        let html = render_markdown_reply_html("hello **world** and `code`", 14);
        assert!(html.contains("<i>(truncated)</i>"));
        assert!(!html.contains("**"));
        assert!(!html.contains("`"));
    }

    #[test]
    fn render_reply_preserves_truncated_fenced_code_blocks() {
        let markdown = "```rust\nlet value = 1;\nlet second = 2;\nlet third = 3;\n```";
        let html = render_markdown_reply_html(markdown, 24);
        assert!(html.contains("<pre><code>"));
        assert!(html.contains("<pre><code>let"));
        assert!(html.contains("<i>(truncated)</i>"));
        assert!(!html.is_empty());
    }

    #[test]
    fn render_reply_uses_visible_length_not_raw_html_length() {
        let markdown = (0..20)
            .map(|i| {
                format!(
                    "[x{i}](https://example.com/{}/{i})",
                    "very-long-path".repeat(12)
                )
            })
            .collect::<Vec<_>>()
            .join(" ");

        let html = markdown_to_telegram_html(&markdown);
        let plain = markdown_to_telegram_plain_text(&markdown);
        assert!(plain.len() < 300);
        assert!(html.len() > 4096);

        let rendered = render_markdown_reply_html(&markdown, 4096);
        assert!(!rendered.contains("<i>(truncated)</i>"));
        assert_eq!(rendered, html);
    }
}
