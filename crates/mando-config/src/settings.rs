//! Config structs matching config.json schema (serde, camelCase).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Root
// ---------------------------------------------------------------------------

/// Root configuration for Mando.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Config {
    pub workspace: String,
    pub features: FeaturesConfig,
    pub channels: ChannelsConfig,
    pub gateway: GatewayConfig,
    pub captain: CaptainConfig,
    pub scout: ScoutConfig,
    pub env: HashMap<String, String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            workspace: "~/.mando/workspace".into(),
            features: FeaturesConfig::default(),
            channels: ChannelsConfig::default(),
            gateway: GatewayConfig::default(),
            captain: CaptainConfig::default(),
            scout: ScoutConfig::default(),
            env: HashMap::new(),
        }
    }
}

impl Config {
    /// Populate runtime-only fields from the `env` section.
    ///
    /// Call after deserialization (in loader and PUT /api/config handler).
    pub fn populate_runtime_fields(&mut self) {
        if let Some(val) = self.env.get("TELEGRAM_MANDO_BOT_TOKEN") {
            self.channels.telegram.token = val.clone();
        }
    }
}

// ---------------------------------------------------------------------------
// Features
// ---------------------------------------------------------------------------

/// Feature flags.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FeaturesConfig {
    pub scout: bool,
    pub setup_dismissed: bool,
    pub claude_code_verified: bool,
}

// ---------------------------------------------------------------------------
// Scout
// ---------------------------------------------------------------------------

/// Per-user scout configuration — interests, user context, and repo summaries.
/// Stored in config.json so it's per-user and gitignored.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ScoutConfig {
    pub interests: crate::workflow_scout::InterestsConfig,
    pub user_context: crate::workflow_scout::UserContextConfig,
}

// ---------------------------------------------------------------------------
// Channels
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ChannelsConfig {
    pub telegram: TelegramConfig,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TelegramConfig {
    pub enabled: bool,
    /// Runtime-only: populated from env.TELEGRAM_*_BOT_TOKEN, not serialized.
    #[serde(skip)]
    pub token: String,
    pub owner: String,
}

// ---------------------------------------------------------------------------
// Gateway
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct GatewayConfig {
    pub dashboard: DashboardConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DashboardConfig {
    pub host: String,
    pub port: u16,
}

impl Default for DashboardConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".into(),
            port: 18791,
        }
    }
}

// ---------------------------------------------------------------------------
// Captain
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CaptainConfig {
    pub auto_schedule: bool,
    pub tick_interval_s: u64,
    pub tz: String,
    pub projects: HashMap<String, ProjectConfig>,
    #[serde(skip)]
    pub task_db_path: String,
    #[serde(skip)]
    pub lockfile_path: String,
    #[serde(skip)]
    pub worker_health_path: String,
}

impl Default for CaptainConfig {
    fn default() -> Self {
        Self {
            auto_schedule: false,
            tick_interval_s: 30,
            tz: iana_time_zone::get_timezone().unwrap_or_else(|_| "UTC".into()),
            projects: HashMap::new(),
            task_db_path: crate::paths::data_dir()
                .join("mando.db")
                .to_string_lossy()
                .into_owned(),
            lockfile_path: crate::paths::data_dir()
                .join("captain.lock")
                .to_string_lossy()
                .into_owned(),
            worker_health_path: crate::paths::state_dir()
                .join("worker-health.json")
                .to_string_lossy()
                .into_owned(),
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ProjectConfig {
    pub name: String,
    pub path: String,
    pub github_repo: Option<String>,
    pub hooks: HashMap<String, String>,
    pub aliases: Vec<String>,
    pub worker_preamble: String,
    pub check_command: String,
    /// Filename of the project logo in `~/.mando/images/` (e.g. `project-mando.png`).
    pub logo: Option<String>,
    /// If set, this project is included in scout QA context with this summary.
    pub scout_summary: String,
    /// Per-project file classification rules for PR triage.
    /// When empty, default rules apply.
    #[serde(default)]
    pub classify_rules: Vec<ClassifyRule>,
}

/// A single file classification rule: category name + glob patterns.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClassifyRule {
    pub category: String,
    pub patterns: Vec<String>,
}

// ---------------------------------------------------------------------------
// CC Worktree Cleanup
// ---------------------------------------------------------------------------
