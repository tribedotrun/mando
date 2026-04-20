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
#[serde(default)]
pub struct ScoutItem {
    pub id: i64,
    pub url: String,
    pub item_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub status: ScoutStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relevance: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quality: Option<i64>,
    pub date_added: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_processed: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub added_by: Option<String>,
    pub error_count: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_published: Option<String>,
    pub rev: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub research_run_id: Option<i64>,
}

impl Default for ScoutItem {
    fn default() -> Self {
        Self {
            id: 0,
            url: String::new(),
            item_type: String::new(),
            title: None,
            status: ScoutStatus::Pending,
            relevance: None,
            quality: None,
            date_added: String::new(),
            date_processed: None,
            added_by: None,
            error_count: 0,
            source_name: None,
            date_published: None,
            rev: super::default_rev(),
            research_run_id: None,
        }
    }
}

/// Status of a research run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ResearchRunStatus {
    #[serde(rename = "running")]
    Running,
    #[serde(rename = "done")]
    Done,
    #[serde(rename = "failed")]
    Failed,
}

impl ResearchRunStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Done => "done",
            Self::Failed => "failed",
        }
    }
}

impl fmt::Display for ResearchRunStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ResearchRunStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "running" => Ok(Self::Running),
            "done" => Ok(Self::Done),
            "failed" => Ok(Self::Failed),
            _ => Err(format!("unknown research run status: {s}")),
        }
    }
}

/// A scout research run record.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ScoutResearchRun {
    pub id: i64,
    pub research_prompt: String,
    pub status: ResearchRunStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    pub added_count: i64,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
    pub rev: i64,
}

impl Default for ScoutResearchRun {
    fn default() -> Self {
        Self {
            id: 0,
            research_prompt: String::new(),
            status: ResearchRunStatus::Running,
            error: None,
            session_id: None,
            added_count: 0,
            created_at: String::new(),
            completed_at: None,
            rev: super::default_rev(),
        }
    }
}
