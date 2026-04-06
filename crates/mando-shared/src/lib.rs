//! mando-shared — shared infrastructure: SSE event bus,
//! helpers, and Telegram HTML formatting.

pub mod bus;
pub mod error;
pub mod helpers;
pub mod retry;
pub mod telegram_format;
pub mod telegram_tables;
pub mod transcript;

// Convenience re-exports.
pub use bus::EventBus;
pub use error::SharedError;
pub use helpers::{load_json_file, pr_short_label, sanitize_path_id, save_json_file};
pub use telegram_format::{
    escape_html, linkify_pr_refs, markdown_to_telegram_html, render_markdown_reply_html,
    split_message, status_icon, TELEGRAM_TEXT_MAX_LEN,
};
