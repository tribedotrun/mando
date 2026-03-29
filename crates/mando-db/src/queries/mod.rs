pub mod analytics;
pub mod ask_history;
pub mod cron;
pub mod journal;
pub mod linear_workpad;
pub mod rebase;
pub mod scout;
pub mod sessions;
pub mod tasks;
mod tasks_row;
pub mod timeline;
pub mod voice;

/// RFC 3339 timestamp for `now() - days`.
pub(crate) fn cutoff_rfc3339(days: i64) -> String {
    let cutoff = time::OffsetDateTime::now_utc() - time::Duration::days(days);
    cutoff
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap()
}
