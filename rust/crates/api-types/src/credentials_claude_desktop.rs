//! Persistent Claude Desktop profile controls. Login state remains owned by Claude.
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClaudeDesktopProfileRequest {
    pub credential_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClaudeDesktopProfileAdoptRequest {
    pub credential_id: i64,
    pub user_data_dir: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ClaudeDesktopProfileState {
    Unconfigured,
    LoginRequired,
    Ready,
    AccountChanged,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClaudeDesktopProfileStatus {
    pub credential_id: i64,
    pub user_data_dir: String,
    pub shared_claude_dir: String,
    /// Last identity recorded by Desktop, not proof that its session is still valid.
    pub account_uuid: Option<String>,
    pub expected_account_uuid: Option<String>,
    pub state: ClaudeDesktopProfileState,
    pub running: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClaudeDesktopSessionSyncResponse {
    pub imported: u32,
    pub eligible: u32,
    pub skipped: u32,
    pub journal_path: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ClaudeDesktopSessionRange {
    Week,
    Month,
    All,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClaudeDesktopSessionImportRequest {
    pub credential_id: i64,
    pub range: ClaudeDesktopSessionRange,
}
