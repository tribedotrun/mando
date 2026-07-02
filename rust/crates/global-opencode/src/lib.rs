//! OpenCode CLI provider boundary.
//!
//! This crate owns OpenCode-specific process invocation and JSON stream parsing
//! so captain stages can route through OpenCode without embedding CLI details in
//! the orchestration state machine.

mod process;
mod stream;

pub use process::{
    ensure_binary_available, spawn_run, terminate_process, OpenCodeRunConfig, StartedOpenCodeRun,
};
pub use stream::{
    initial_stream_lines, normalize_event_lines, result_stream_line, OpenCodeCompletion,
    OpenCodeEvent, OpenCodeStreamState,
};
