//! I/O wrappers around external systems.

use global_infra::retry::RetryConfig;

pub mod queries;

/// Shared retry config for `gh` CLI operations.
pub(crate) fn gh_retry_config() -> RetryConfig {
    RetryConfig::default()
}

pub mod captain_lock;
pub mod cc_session;
pub(crate) mod gh_run;
pub mod git;
pub mod github;
pub mod github_pr;
pub mod headless_cc;
pub mod health_store;
pub mod hooks;
pub mod item_lock;
pub mod ops_log;
pub mod pid_lookup;
pub mod pid_registry;
pub mod process_manager;
pub mod session_terminate;
pub mod task_cleanup;
pub mod task_store;
pub mod timeline_store;
pub mod transcript;
