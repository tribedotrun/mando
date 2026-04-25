//! Runtime layer — process orchestration and dashboard API handlers.

pub mod article;
pub mod daemon;
pub mod dashboard;
mod dashboard_support;
pub mod process;
pub mod qa;
mod qa_parse;
pub mod research;

pub use daemon::ScoutRuntime;
