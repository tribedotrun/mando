//! mando-captain — captain tick loop, task engine, worker management,
//! and the deterministic state machine.
//!
//! Tier discipline: types -> config -> io -> service -> runtime

pub mod config;
pub mod io;
pub mod runtime;
pub mod service;
pub mod types;

pub use runtime::worker_exit::{watch_worker_exit, WORKER_EXIT_SIGNAL};
pub use types::*;
