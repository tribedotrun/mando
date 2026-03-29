//! mando-shared — shared infrastructure: cron service, SSE event bus,
//! helpers, and Telegram HTML formatting.

pub mod bus;
pub mod cron;
pub mod helpers;
pub mod quiet_mode;
pub mod retry;
pub mod telegram_format;
pub mod telegram_tables;
pub mod transcript;

// Convenience re-exports.
pub use bus::EventBus;
pub use cron::api::{
    add_cron_job, list_cron_jobs, parse_schedule, remove_cron_job, toggle_cron_job,
};
pub use cron::parser::CronExpr;
pub use cron::scheduler::compute_next_run;
pub use cron::service::CronService;
pub use cron::store::{load_store, save_store, CronStore};
pub use helpers::{load_json_file, pr_short_label, sanitize_path_id, save_json_file};
pub use telegram_format::{
    convert_md_tables, escape_html, format_item_line, format_item_line_with_repo, hyperlink,
    linear_hyperlink, linkify_linear_refs, linkify_pr_refs, markdown_to_telegram_html,
    markdown_to_telegram_plain_text, pr_hyperlink, render_markdown_reply_html,
    repo_slug_from_remote, split_message, status_icon, TELEGRAM_TEXT_MAX_LEN,
};
