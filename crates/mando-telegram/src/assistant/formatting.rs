//! Card formatting and inline keyboard builders for the assistant bot.

use mando_shared::telegram_format::{
    escape_html, markdown_to_telegram_html, render_markdown_reply_html, TELEGRAM_TEXT_MAX_LEN,
};
use serde_json::{json, Value};

/// Type-based icons for scout item types.
const TYPE_ICONS: &[(&str, &str)] = &[
    ("youtube", "\u{1f3ac}"),
    ("github", "\u{1f4bb}"),
    ("arxiv", "\u{1f4c4}"),
    ("blog", "\u{1f4dd}"),
    ("other", "\u{1f4dd}"),
];

fn type_icon(item_type: &str) -> &'static str {
    TYPE_ICONS
        .iter()
        .find(|(t, _)| *t == item_type)
        .map(|(_, icon)| *icon)
        .unwrap_or("\u{1f4c4}")
}

/// Strip the metadata prefix that `process.rs` prepends to summaries.
///
/// The prefix contains lines like `# Title`, `**Source**: ...`,
/// `**Type**: ...`, `**Published**: ...`, `**Relevance**: ...` followed by
/// a blank line. Everything after the first blank line following that block
/// is the actual summary content.
fn strip_summary_metadata(summary: &str) -> &str {
    let mut found_metadata = false;

    for line in summary.lines() {
        let trimmed = line.trim();
        let is_metadata_field = trimmed.starts_with("**Source**:")
            || trimmed.starts_with("**Type**:")
            || trimmed.starts_with("**Published**:")
            || trimmed.starts_with("**Relevance**:");
        // Headings and blanks are skippable within a metadata block, but only
        // the known **Field**: lines confirm that we're actually in one.
        let is_skippable = trimmed.is_empty() || trimmed.starts_with('#') || is_metadata_field;

        if is_skippable {
            if is_metadata_field {
                found_metadata = true;
            }
        } else if found_metadata {
            // First content line after metadata -- return from here.
            let offset = line.as_ptr() as usize - summary.as_ptr() as usize;
            return &summary[offset..];
        } else {
            // Non-metadata line with no preceding metadata -- no prefix to strip.
            return summary;
        }
    }

    // Return empty only when confirmed metadata was found; otherwise
    // the summary was just headings/blanks and should be kept as-is.
    if found_metadata {
        ""
    } else {
        summary
    }
}

/// Parse an RFC 3339 date string into a short "Mon DD" display form.
fn format_short_date(rfc3339: &str) -> Option<String> {
    // RFC 3339 starts with YYYY-MM-DD; parse the date portion.
    let date_part = rfc3339.get(..10)?;
    let mut parts = date_part.splitn(3, '-');
    let _year: u16 = parts.next()?.parse().ok()?;
    let month: u8 = parts.next()?.parse().ok()?;
    let day: u8 = parts.next()?.parse().ok()?;
    let mon = match month {
        1 => "Jan",
        2 => "Feb",
        3 => "Mar",
        4 => "Apr",
        5 => "May",
        6 => "Jun",
        7 => "Jul",
        8 => "Aug",
        9 => "Sep",
        10 => "Oct",
        11 => "Nov",
        12 => "Dec",
        _ => return None,
    };
    Some(format!("{mon} {day}"))
}

/// Format a scout item as a swipe card (HTML).
pub fn format_swipe_card(item: &Value, summary: Option<&str>) -> String {
    let title = item["title"].as_str().unwrap_or("Untitled");
    let url = item["url"].as_str().unwrap_or("");
    let item_type = item["item_type"].as_str().unwrap_or("other");
    let icon = type_icon(item_type);

    let scores = match (item["relevance"].as_i64(), item["quality"].as_i64()) {
        (Some(r), Some(q)) => format!("R:{r}\u{00b7}Q:{q}"),
        _ => "\u{2014}".into(),
    };

    let source = item["source_name"].as_str().filter(|s| !s.is_empty());
    let src_visible = source.map(|s| format!(" \u{00b7} {s}")).unwrap_or_default();
    let src_part = source
        .map(|s| format!(" \u{00b7} {}", escape_html(s)))
        .unwrap_or_default();

    let date_published = item["date_published"].as_str().filter(|s| !s.is_empty());
    let date_part = date_published
        .map(|d| format!(" \u{00b7} {}", escape_html(d)))
        .unwrap_or_default();

    let date_added = item["date_added"].as_str().and_then(format_short_date);
    let added_part = date_added
        .as_deref()
        .map(|d| format!(" \u{00b7} Added: {}", escape_html(d)))
        .unwrap_or_default();
    let added_visible = date_added
        .as_deref()
        .map(|d| format!(" \u{00b7} Added: {d}"))
        .unwrap_or_default();

    let mut text = format!(
        "{icon} {scores}{src_part}{date_part}{added_part}\n<a href=\"{}\">{}</a>",
        escape_html(url),
        escape_html(title),
    );

    if let Some(s) = summary {
        let stripped = strip_summary_metadata(s);
        let date_visible = date_published
            .map(|d| format!(" \u{00b7} {d}"))
            .unwrap_or_default();
        let header_visible =
            format!("{icon} {scores}{src_visible}{date_visible}{added_visible}\n{title}");
        let available = TELEGRAM_TEXT_MAX_LEN.saturating_sub(header_visible.len() + "\n\n".len());
        let rendered = render_markdown_reply_html(stripped, available);
        if !rendered.is_empty() {
            text.push_str(&format!("\n\n{rendered}"));
        }
    }

    text
}

/// Render the summary preview used by list surfaces.
pub fn render_summary_preview(summary: &str) -> String {
    let preview = summary
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty()
                || trimmed.starts_with('#')
                || trimmed.starts_with("**") && trimmed.contains("**:")
            {
                None
            } else {
                Some(trimmed)
            }
        })
        .take(4)
        .collect::<Vec<_>>()
        .join("\n");

    markdown_to_telegram_html(&preview)
}

/// Build the inline keyboard for a scout swipe card.
///
/// Layout:
/// ```text
/// [📖 Read] [💬 Ask] [Next ▶]
/// [⭐ Save] [📦 Archive] [⚙️ Act] [🗑]
/// ```
pub fn swipe_card_kb(item_id: i64, telegraph_url: Option<&str>) -> Value {
    let mut top_row = Vec::new();
    if let Some(url) = telegraph_url.filter(|u| !u.is_empty()) {
        top_row.push(json!({"text": "\u{1f4d6} Read", "url": url}));
    } else {
        top_row
            .push(json!({"text": "\u{1f4d6} Read", "callback_data": format!("dg:read:{item_id}")}));
    }
    top_row.push(json!({"text": "\u{1f4ac} Ask", "callback_data": format!("dg:ask:{item_id}")}));
    top_row.push(json!({"text": "Next \u{25b6}", "callback_data": format!("dg:next:{item_id}")}));

    let action_row = json!([
        {"text": "\u{2b50} Save", "callback_data": format!("dg:save:{item_id}")},
        {"text": "\u{1f4e6} Archive", "callback_data": format!("dg:archive:{item_id}")},
        {"text": "\u{2699}\u{fe0f} Act", "callback_data": format!("dg:act:{item_id}")},
        {"text": "\u{1f5d1}", "callback_data": format!("dg:rm:{item_id}")},
    ]);
    json!({"inline_keyboard": [top_row, action_row]})
}

/// Build list keyboard with item selector buttons and pagination nav.
///
/// Each item gets a positional button (callback `dg:show:{id}`), arranged in
/// rows of `items_per_row`. Pagination nav (Prev/Next) is appended below.
///
/// `start_offset`: global position of the first item on this page (0-indexed).
/// `prefix`: `"dg:page"` for summary list, `"dg:cpage"` for compact list.
pub fn list_kb(
    item_ids: &[i64],
    page: usize,
    total_pages: usize,
    status: &str,
    prefix: &str,
    items_per_row: usize,
    start_offset: usize,
) -> Value {
    let mut rows: Vec<Value> = Vec::new();

    // Item selector buttons in chunks — labeled by global position
    for (chunk_idx, chunk) in item_ids.chunks(items_per_row).enumerate() {
        let row: Vec<Value> = chunk
            .iter()
            .enumerate()
            .map(|(i, id)| {
                let pos = start_offset + chunk_idx * items_per_row + i + 1;
                json!({"text": format!("{pos}"), "callback_data": format!("dg:show:{id}")})
            })
            .collect();
        rows.push(json!(row));
    }

    // Pagination nav row
    let mut nav = Vec::new();
    if page > 0 {
        nav.push(json!({
            "text": "\u{25c0} Prev",
            "callback_data": format!("{prefix}:{}:{status}", page - 1),
        }));
    }
    if page + 1 < total_pages {
        nav.push(json!({
            "text": "Next \u{25b6}",
            "callback_data": format!("{prefix}:{}:{status}", page + 1),
        }));
    }
    if !nav.is_empty() {
        rows.push(json!(nav));
    }

    if rows.is_empty() {
        json!(null)
    } else {
        json!({"inline_keyboard": rows})
    }
}

/// Build the inline keyboard for Telegraph article reading.
///
/// `[📖 Read on Telegraph] [◀ Summary] [⭐ Save] [📦 Archive]`
pub fn telegraph_read_kb(item_id: i64, url: &str) -> Value {
    let link_row = json!([
        {"text": "\u{1f4d6} Read on Telegraph", "url": url},
    ]);
    let action_row = json!([
        {"text": "\u{25c0} Summary", "callback_data": format!("dg:show:{item_id}")},
        {"text": "\u{2b50} Save", "callback_data": format!("dg:save:{item_id}")},
        {"text": "\u{1f4e6} Archive", "callback_data": format!("dg:archive:{item_id}")},
    ]);
    json!({"inline_keyboard": [link_row, action_row]})
}

/// Build a project picker keyboard for the Act flow.
///
/// One button per project: `dg:actpick:{item_id}:{project_name}`
pub fn act_project_picker_kb(item_id: i64, project_names: &[String]) -> Value {
    let buttons: Vec<Value> = project_names
        .iter()
        .map(|name| json!({"text": name, "callback_data": format!("dg:actpick:{item_id}:{name}")}))
        .collect();
    json!({"inline_keyboard": [buttons]})
}

/// Build the inline keyboard for the Act prompt step.
///
/// `[▶ Skip — use default]`
pub fn act_prompt_kb(item_id: i64) -> Value {
    json!({"inline_keyboard": [[
        {"text": "\u{25b6} Skip \u{2014} use default", "callback_data": format!("dg:actskip:{item_id}")},
    ]]})
}

/// Build the inline keyboard for an active Q&A session.
///
/// `[◀ Summary] [⏹ End session]`
pub fn qa_session_kb(item_id: i64) -> Value {
    json!({"inline_keyboard": [[
        {"text": "\u{25c0} Summary", "callback_data": format!("dg:show:{item_id}")},
        {"text": "\u{23f9} End session", "callback_data": format!("dg:endqa:{item_id}")},
    ]]})
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Extract the `inline_keyboard` rows from a keyboard Value, failing the
    /// test with a clear structural message if the shape is wrong.
    fn kb_rows(kb: &Value) -> &Vec<Value> {
        kb["inline_keyboard"]
            .as_array()
            .expect("keyboard must have inline_keyboard array")
    }

    /// Extract a row (array) from a Value, failing with a clear message.
    fn row_array<'a>(row: &'a Value, label: &str) -> &'a Vec<Value> {
        row.as_array()
            .unwrap_or_else(|| panic!("{label} must be an array"))
    }

    /// Extract a string field, failing with a clear message.
    fn field_str<'a>(v: &'a Value, label: &str) -> &'a str {
        v.as_str()
            .unwrap_or_else(|| panic!("{label} must be a string"))
    }

    #[test]
    fn type_icon_lookup() {
        assert_eq!(type_icon("youtube"), "\u{1f3ac}");
        assert_eq!(type_icon("github"), "\u{1f4bb}");
        assert_eq!(type_icon("unknown"), "\u{1f4c4}");
    }

    #[test]
    fn swipe_card_kb_has_action_row() {
        let kb = swipe_card_kb(42, None);
        let rows = kb_rows(&kb);
        assert_eq!(rows.len(), 2);
        // Top row: Read, Ask, Next = 3 buttons
        let top_row = row_array(&rows[0], "swipe card top row");
        assert_eq!(top_row.len(), 3);
        assert!(field_str(
            &top_row[0]["callback_data"],
            "swipe card read button callback"
        )
        .contains("read"));
        // Action row: Save, Archive, Act, Delete
        let action_row = row_array(&rows[1], "swipe card action row");
        assert_eq!(action_row.len(), 4);
    }

    #[test]
    fn swipe_card_kb_youtube_telegraph() {
        let kb = swipe_card_kb(42, Some("https://telegra.ph/Test-42"));
        let rows = kb_rows(&kb);
        let top_row = row_array(&rows[0], "swipe card top row");
        assert_eq!(top_row.len(), 3);
        assert_eq!(
            field_str(&top_row[0]["url"], "telegraph read url"),
            "https://telegra.ph/Test-42"
        );
    }

    #[test]
    fn swipe_card_kb_youtube_no_telegraph_uses_callback() {
        let kb = swipe_card_kb(42, None);
        let rows = kb_rows(&kb);
        let top_row = row_array(&rows[0], "swipe card top row");
        assert!(field_str(&top_row[0]["callback_data"], "fallback read callback").contains("read"));
    }

    #[test]
    fn qa_session_kb_has_end_button() {
        let kb = qa_session_kb(42);
        let rows = kb_rows(&kb);
        assert_eq!(rows.len(), 1);
        let row = row_array(&rows[0], "qa session row");
        assert_eq!(row.len(), 2);
        assert!(field_str(&row[1]["callback_data"], "qa end button callback").contains("endqa"));
    }

    #[test]
    fn render_summary_preview_converts_markdown() {
        let preview = render_summary_preview(
            "# Heading\n**bold**\n- item one\nUse `code`\nhttps://example.com/docs",
        );

        assert!(preview.contains("<b>bold</b>"));
        assert!(preview.contains("\u{2022} item one"));
        assert!(preview.contains("<code>code</code>"));
        assert!(preview.contains("<a href=\"https://example.com/docs\">"));
        assert!(!preview.contains("# Heading"));
    }

    #[test]
    fn format_card_renders_markdown_summary() {
        let item = json!({
            "id": 1,
            "title": "Test",
            "url": "https://example.com",
            "item_type": "other",
            "relevance": 85,
            "quality": 70,
        });

        let card = format_swipe_card(&item, Some("**bold**\n- `code`"));
        assert!(card.contains("<b>bold</b>"));
        assert!(card.contains("<code>code</code>"));
        assert!(!card.contains("**bold**"));
    }

    #[test]
    fn list_kb_no_items_single_page() {
        let kb = list_kb(&[], 0, 1, "saved", "dg:page", 5, 0);
        assert!(kb.is_null());
    }

    #[test]
    fn list_kb_items_only_no_pagination() {
        let kb = list_kb(&[10, 20, 30], 0, 1, "all", "dg:page", 5, 0);
        let rows = kb_rows(&kb);
        assert_eq!(rows.len(), 1); // one row of item buttons, no nav
        let item_row = row_array(&rows[0], "list_kb item row");
        assert_eq!(item_row.len(), 3);
        assert_eq!(field_str(&item_row[0]["text"], "first button text"), "1");
        assert!(
            field_str(&item_row[0]["callback_data"], "first button callback")
                .contains("dg:show:10")
        );
    }

    #[test]
    fn list_kb_with_pagination() {
        let kb = list_kb(&[1, 2, 3], 0, 3, "saved", "dg:page", 5, 0);
        let rows = kb_rows(&kb);
        assert_eq!(rows.len(), 2); // item row + nav row
        let nav_row = row_array(&rows[1], "list_kb nav row");
        assert_eq!(nav_row.len(), 1); // only Next on first page
    }

    #[test]
    fn list_kb_chunked_rows() {
        let ids: Vec<i64> = (1..=7).collect();
        let kb = list_kb(&ids, 1, 3, "all", "dg:cpage", 5, 5);
        let rows = kb_rows(&kb);
        // 5 + 2 items = 2 item rows, plus 1 nav row (Prev + Next)
        assert_eq!(rows.len(), 3);
        assert_eq!(row_array(&rows[0], "first item row").len(), 5);
        assert_eq!(row_array(&rows[1], "second item row").len(), 2);
        assert_eq!(row_array(&rows[2], "nav row").len(), 2); // Prev + Next
    }

    #[test]
    fn format_card_basic() {
        let item = json!({
            "id": 1,
            "title": "Test",
            "url": "https://example.com",
            "item_type": "other",
            "relevance": 85,
            "quality": 70,
        });
        let card = format_swipe_card(&item, None);
        assert!(card.contains("R:85"));
        assert!(card.contains("Test"));
    }

    #[test]
    fn format_card_strips_metadata_prefix() {
        let item = json!({
            "id": 1,
            "title": "AI News",
            "url": "https://example.com",
            "item_type": "blog",
            "relevance": 90,
            "quality": 80,
            "source_name": "TechBlog",
        });
        let summary = "# AI News\n\n\
                        **Source**: TechBlog\n\
                        **Type**: blog\n\
                        **Published**: 2026-04-01\n\
                        **Relevance**: 90/100 | **Quality**: 80/100\n\n\
                        This is the actual summary content.\n\
                        It has multiple lines.";
        let card = format_swipe_card(&item, Some(summary));
        // Should contain the actual summary content
        assert!(card.contains("actual summary content"));
        // Should NOT contain the duplicated metadata lines rendered as HTML
        assert!(!card.contains("Source"));
        assert!(!card.contains("Relevance"));
        assert!(!card.contains("90/100"));
    }

    #[test]
    fn format_card_shows_date_added() {
        let item = json!({
            "id": 1,
            "title": "Test",
            "url": "https://example.com",
            "item_type": "other",
            "relevance": 85,
            "quality": 70,
            "date_added": "2026-04-12T10:30:00Z",
        });
        let card = format_swipe_card(&item, None);
        assert!(card.contains("Added: Apr 12"));
    }

    #[test]
    fn format_card_no_date_added_when_missing() {
        let item = json!({
            "id": 1,
            "title": "Test",
            "url": "https://example.com",
            "item_type": "other",
            "relevance": 85,
            "quality": 70,
        });
        let card = format_swipe_card(&item, None);
        assert!(!card.contains("Added:"));
    }

    #[test]
    fn strip_summary_metadata_removes_prefix() {
        let summary = "# Title\n\n\
                        **Source**: Blog\n\
                        **Type**: blog\n\
                        **Published**: 2026-04-01\n\
                        **Relevance**: 90/100 | **Quality**: 80/100\n\n\
                        Actual content here.";
        let stripped = strip_summary_metadata(summary);
        assert_eq!(stripped, "Actual content here.");
    }

    #[test]
    fn strip_summary_metadata_no_prefix() {
        let summary = "Just plain content.\nNo metadata here.";
        let stripped = strip_summary_metadata(summary);
        assert_eq!(stripped, summary);
    }

    #[test]
    fn strip_summary_metadata_only_metadata() {
        let summary = "# Title\n**Source**: Blog\n**Type**: blog";
        let stripped = strip_summary_metadata(summary);
        assert_eq!(stripped, "");
    }

    #[test]
    fn strip_summary_metadata_preserves_content_headings() {
        let summary = "## Key insights\n\nSome valuable content here.";
        let stripped = strip_summary_metadata(summary);
        assert_eq!(stripped, summary);
    }

    #[test]
    fn strip_summary_metadata_heading_only_preserved() {
        let summary = "# Just a heading";
        let stripped = strip_summary_metadata(summary);
        assert_eq!(stripped, summary);
    }

    #[test]
    fn format_short_date_valid() {
        assert_eq!(
            format_short_date("2026-04-12T10:30:00Z"),
            Some("Apr 12".into())
        );
        assert_eq!(format_short_date("2026-01-01"), Some("Jan 1".into()));
        assert_eq!(
            format_short_date("2025-12-25T00:00:00+05:00"),
            Some("Dec 25".into())
        );
    }

    #[test]
    fn format_short_date_invalid() {
        assert_eq!(format_short_date("not-a-date"), None);
        assert_eq!(format_short_date(""), None);
    }
}
