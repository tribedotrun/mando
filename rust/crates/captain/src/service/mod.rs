//! Pure business logic — no I/O, no subprocess, no file writes.

pub mod deterministic;
mod deterministic_helpers;
pub mod dispatch_logic;
pub mod lifecycle;
pub mod merge_logic;
pub mod review_marshal;
pub mod spawn_logic;
pub(crate) mod text;
pub mod tick_logic;
pub mod tick_summary;
pub mod triage;
pub mod worker_context;
