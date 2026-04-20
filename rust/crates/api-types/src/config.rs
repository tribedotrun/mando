use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClassifyRule {
    pub category: String,
    pub patterns: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProjectConfig {
    pub name: String,
    pub path: String,
    pub github_repo: Option<String>,
    pub logo: Option<String>,
    pub aliases: Vec<String>,
    pub hooks: HashMap<String, String>,
    pub worker_preamble: String,
    pub scout_summary: String,
    pub check_command: String,
    pub classify_rules: Vec<ClassifyRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FeaturesConfig {
    pub scout: bool,
    pub setup_dismissed: bool,
    pub claude_code_verified: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TelegramConfig {
    pub enabled: bool,
    pub owner: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChannelsConfig {
    pub telegram: TelegramConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DashboardConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GatewayConfig {
    pub dashboard: DashboardConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InterestsConfig {
    pub high: Vec<String>,
    pub low: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UserContextConfig {
    pub role: String,
    pub known_domains: Vec<String>,
    pub explain_domains: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScoutConfig {
    pub interests: InterestsConfig,
    pub user_context: UserContextConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CaptainConfig {
    pub auto_schedule: bool,
    pub auto_merge: bool,
    pub max_concurrent_workers: Option<usize>,
    pub tick_interval_s: u64,
    pub tz: String,
    pub default_terminal_agent: String,
    pub claude_terminal_args: String,
    pub codex_terminal_args: String,
    pub projects: HashMap<String, ProjectConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UiConfig {
    pub open_at_login: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MandoConfig {
    pub workspace: String,
    pub ui: UiConfig,
    pub features: FeaturesConfig,
    pub channels: ChannelsConfig,
    pub gateway: GatewayConfig,
    pub captain: CaptainConfig,
    pub scout: ScoutConfig,
    pub env: HashMap<String, String>,
}
