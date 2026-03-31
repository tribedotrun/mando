//! mando-scout — scout domain crate.
//!
//! Layers:
//! - `biz/` — pure business logic (URL classification, formatting)
//! - `io/` — database, HTTP, filesystem I/O
//! - `runtime/` — process orchestration and dashboard API handlers

pub mod biz;
pub mod io;
pub mod runtime;

// Convenience re-exports.
pub use biz::url_detect::{derive_source_label, UrlType};
pub use io::db::{ListQuery, ListResult, ScoutDb, SessionRow};
pub use io::file_store::{
    article_path, content_path, delete_item_files, read_content, read_summary, summary_path,
    telegraph_cache_path, write_article, write_content, write_summary,
};
pub use runtime::dashboard::{
    act_on_scout_item, add_scout_item, delete_scout_item, ensure_scout_article, get_scout_article,
    get_scout_item, list_scout_items, process_scout, update_scout_status,
};
pub use runtime::process::process_item;
