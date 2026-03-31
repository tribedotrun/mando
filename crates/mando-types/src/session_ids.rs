//! Session ID container — stores worker, review, and clarifier CC session IDs as JSON.

use serde::{Deserialize, Serialize};
use tracing::warn;

/// Session IDs for the four types of CC sessions a task can have.
/// Stored as a JSON TEXT column in SQLite.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionIds {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worker: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clarifier: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub merge: Option<String>,
}

impl SessionIds {
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".into())
    }

    pub fn from_json(s: &str) -> Self {
        match serde_json::from_str(s) {
            Ok(v) => v,
            Err(e) => {
                warn!(input = %s, error = %e, "SessionIds::from_json failed to parse");
                Self::default()
            }
        }
    }
}
