//! Cron job domain types.

use serde::{Deserialize, Serialize};

/// Schedule definition for a cron job.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CronSchedule {
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub every_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expr: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tz: Option<String>,
}

impl Default for CronSchedule {
    fn default() -> Self {
        Self {
            kind: "every".into(),
            at_ms: None,
            every_ms: None,
            expr: None,
            tz: None,
        }
    }
}

/// What to do when the cron job runs.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CronPayload {
    #[serde(default = "default_payload_kind")]
    pub kind: String,
    #[serde(default)]
    pub message: String,
    #[serde(default)]
    pub deliver: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
}

fn default_payload_kind() -> String {
    "agent_turn".into()
}

impl Default for CronPayload {
    fn default() -> Self {
        Self {
            kind: default_payload_kind(),
            message: String::new(),
            deliver: false,
            channel: None,
            to: None,
        }
    }
}

/// Runtime state of a cron job.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CronState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_run_at_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_run_at_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

/// A scheduled cron job.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CronJob {
    pub id: String,
    pub name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub schedule: CronSchedule,
    #[serde(default)]
    pub payload: CronPayload,
    #[serde(default)]
    pub state: CronState,
    #[serde(default)]
    pub created_at_ms: i64,
    #[serde(default)]
    pub updated_at_ms: i64,
    #[serde(default)]
    pub delete_after_run: bool,
    #[serde(rename = "type", default = "default_job_type")]
    pub job_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default = "default_timeout")]
    pub timeout_s: i64,
}

fn default_true() -> bool {
    true
}

fn default_job_type() -> String {
    "system".into()
}

fn default_timeout() -> i64 {
    1200
}
