//! Settings pure business logic.

mod config_apply;
mod workflow_mode;

pub use config_apply::build_config_apply_outcome;
pub use workflow_mode::{apply_scout_workflow_mode_overrides, apply_workflow_mode_overrides};
