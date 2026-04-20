pub(crate) mod dashboard_payloads;
pub mod error;
pub mod scout;

pub use error::{find_scout_error, ScoutError};
pub use scout::{ResearchRunStatus, ScoutItem, ScoutResearchRun, ScoutStatus};

pub(crate) fn default_rev() -> i64 {
    1
}
