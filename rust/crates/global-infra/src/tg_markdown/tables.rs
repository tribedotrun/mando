//! Markdown table → Telegram text conversion.
//!
//! Detects pipe-delimited tables in markdown and renders them as vertical
//! cards suitable for Telegram's limited formatting.

use regex::Regex;
use std::sync::LazyLock;

static TABLE_SEP_RE: LazyLock<Regex> =
    LazyLock::new(
        || match Regex::new(r"^\|?\s*:?-{2,}:?\s*(\|\s*:?-{2,}:?\s*)+\|?\s*$") {
            Ok(re) => re,
            Err(e) => crate::unrecoverable!("regex compile failed", e),
        },
    );

/// Parse a pipe-delimited markdown table row into cell strings.
fn parse_table_row(line: &str) -> Option<Vec<String>> {
    let stripped = line.trim();
    if !stripped.contains('|') {
        return None;
    }
    let inner = stripped.trim_matches('|');
    let cells: Vec<String> = inner.split('|').map(|c| c.trim().to_string()).collect();
    Some(cells)
}

/// Render table rows as vertical cards (bold headers + bullet key:value pairs).
fn render_table_rows(rows: &[Vec<String>]) -> String {
    if rows.is_empty() {
        return String::new();
    }
    let header = &rows[0];
    let data_rows = &rows[1..];
    if data_rows.is_empty() {
        return format!("**{}**", header.join(" | "));
    }
    let ncols = header.len();
    // Comparison table: first header cell is empty, row labels in col 0.
    let is_comparison = header[0].trim().is_empty() && ncols >= 3;
    if is_comparison {
        let cards: Vec<String> = (1..ncols)
            .map(|ci| {
                let mut lines = vec![format!("**{}**", header[ci])];
                for row in data_rows {
                    let key = row.first().map(|s| s.as_str()).unwrap_or("");
                    let val = row.get(ci).map(|s| s.as_str()).unwrap_or("");
                    lines.push(format!("• {key}: {val}"));
                }
                lines.join("\n")
            })
            .collect();
        return cards.join("\n\n");
    }
    // Row-oriented: card per data row.
    let cards: Vec<String> = data_rows
        .iter()
        .map(|row| {
            let mut lines: Vec<String> = Vec::new();
            for (ci, cell) in row.iter().enumerate() {
                if ci == 0 {
                    lines.push(format!("**{cell}**"));
                } else {
                    let label = header.get(ci).map(|s| s.as_str()).unwrap_or("");
                    lines.push(format!("• {label}: {cell}"));
                }
            }
            lines.join("\n")
        })
        .collect();
    cards.join("\n\n")
}

/// Detect markdown pipe-delimited tables and convert to row-by-row text.
pub fn convert_md_tables(text: &str) -> String {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut result: Vec<String> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if TABLE_SEP_RE.is_match(lines[i].trim()) && i > 0 {
            if let Some(header) = parse_table_row(lines[i - 1]) {
                if header.len() >= 2 {
                    let ncols = header.len();
                    let mut rows: Vec<Vec<String>> = vec![header];
                    let mut j = i + 1;
                    while j < lines.len() {
                        if let Some(cells) = parse_table_row(lines[j]) {
                            if cells.len() >= 2 && cells.len() <= ncols {
                                let mut padded = cells;
                                padded.resize(ncols, String::new());
                                rows.push(padded);
                                j += 1;
                                continue;
                            }
                        }
                        break;
                    }
                    // Replace the header line (already pushed) with rendered table.
                    if let Some(last) = result.last_mut() {
                        *last = render_table_rows(&rows);
                    }
                    i = j;
                    continue;
                }
            }
        }
        result.push(lines[i].to_string());
        i += 1;
    }
    result.join("\n")
}
