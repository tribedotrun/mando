//! Scout domain types — items and their statuses.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Status of a scout item.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ScoutStatus {
    #[serde(rename = "pending")]
    Pending,
    #[serde(rename = "fetched")]
    Fetched,
    #[serde(rename = "processed")]
    Processed,
    #[serde(rename = "saved")]
    Saved,
    #[serde(rename = "archived")]
    Archived,
    #[serde(rename = "error")]
    Error,
}

impl ScoutStatus {
    /// String representation matching the DB/serde values.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Fetched => "fetched",
            Self::Processed => "processed",
            Self::Saved => "saved",
            Self::Archived => "archived",
            Self::Error => "error",
        }
    }
}

impl fmt::Display for ScoutStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ScoutStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "pending" => Ok(Self::Pending),
            "fetched" => Ok(Self::Fetched),
            "processed" => Ok(Self::Processed),
            "saved" => Ok(Self::Saved),
            "archived" => Ok(Self::Archived),
            "error" => Ok(Self::Error),
            _ => Err(format!("unknown scout status: {s}")),
        }
    }
}

/// A scout item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoutItem {
    pub id: i64,
    pub url: String,
    pub item_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub status: ScoutStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relevance: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub quality: Option<i64>,
    pub date_added: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date_processed: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub added_by: Option<String>,
    #[serde(default)]
    pub error_count: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date_published: Option<String>,
}
