//! Markdown-to-Telegram-HTML converter and visible-length truncating renderer.
//!
//! Lives in `global-infra` so any crate (captain biz tier, transport-tg)
//! can render LLM-authored markdown into Telegram-safe HTML without the
//! biz tier reaching into the transport tier.

mod markdown;
mod render;
mod tables;

pub use markdown::{markdown_to_telegram_html, markdown_to_telegram_plain_text};
pub use render::{render_markdown_reply_html, TELEGRAM_TEXT_MAX_LEN};
pub use tables::convert_md_tables;
