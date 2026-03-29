//! I/O wrappers around external systems.

use mando_shared::retry::RetryConfig;

/// Shared retry config for `gh` CLI operations.
pub(crate) fn gh_retry_config() -> RetryConfig {
    RetryConfig::default()
}

pub mod cc_session;
pub mod evidence;
pub mod git;
pub mod github;
pub mod github_pr;
pub mod headless_cc;
pub mod health_store;
pub mod hooks;
pub mod item_lock;
pub mod journal;
pub mod journal_types;
pub mod linear;
pub mod ops_log;
pub mod process_manager;
pub mod session_db;
pub mod task_cleanup;
pub mod task_db;
pub mod task_store;
pub mod timeline_store;
pub mod transcript;
