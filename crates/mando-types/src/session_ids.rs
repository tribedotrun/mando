//! Session ID container — stores worker, review, clarifier, and ask CC session IDs as JSON.

use serde::{Deserialize, Serialize};

/// Session IDs for the five types of CC sessions a task can have.
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ask: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub advisor: Option<String>,
}

impl SessionIds {
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".into())
    }

    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }
}
