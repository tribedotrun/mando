pub mod scout;

pub use scout::{ResearchRunStatus, ScoutItem, ScoutResearchRun, ScoutStatus};

pub(crate) fn default_rev() -> i64 {
    1
}
