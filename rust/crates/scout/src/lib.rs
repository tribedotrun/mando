mod config;
mod io;
mod runtime;
mod service;
mod types;

pub use types::*;

pub use io::db::{ListQuery, ListResult, ScoutDb, SessionRow};
pub use io::file_store::{
    content_path, delete_item_files, read_content, telegraph_cache_path, write_content,
};
pub use io::queries::scout::{item_titles, reset_stale_fetched};
pub use io::queries::scout_research::reset_stale_running;
pub use runtime::dashboard::{
    act_on_scout_item, add_scout_item, apply_scout_item_command, bulk_apply_scout_item_command,
    bulk_delete_scout_items, delete_scout_item, ensure_scout_article, get_scout_article,
    get_scout_item, list_scout_items, process_scout, scrape_with_firecrawl,
};
pub use runtime::process::process_item;
pub use runtime::qa::session_manager_from_workflow;
pub use runtime::ScoutRuntime;
pub use service::lifecycle::ScoutItemCommand;
pub use service::url_detect::{derive_source_label, UrlType};
