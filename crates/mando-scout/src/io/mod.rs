//! I/O layer — database, HTTP fetching, and filesystem operations.

pub mod content_fetch;
pub mod db;
pub mod file_store;
pub mod telegraph;
pub mod yt_dlp;

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Strip all HTML tags from a string, keeping only the text content.
pub(crate) fn strip_html_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for ch in s.chars() {
        if ch == '<' {
            in_tag = true;
        } else if ch == '>' {
            in_tag = false;
        } else if !in_tag {
            out.push(ch);
        }
    }
    out
}
