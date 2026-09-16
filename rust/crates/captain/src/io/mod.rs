//! I/O wrappers around external systems.

pub mod queries;

pub mod captain_lock;
pub mod cc_session;
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
pub(crate) mod worktree_bootstrap;
