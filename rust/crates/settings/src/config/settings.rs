//! Config structs matching config.json schema (serde, camelCase).

use api_types::TerminalAgent;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Root
// ---------------------------------------------------------------------------

/// Root configuration for Mando.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub workspace: String,
    pub ui: UiConfig,
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
            ui: UiConfig::default(),
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
        self.captain.populate_runtime_paths();
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiConfig {
    pub open_at_login: bool,
}

// ---------------------------------------------------------------------------
// Features
// ---------------------------------------------------------------------------

/// Feature flags.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
#[serde(rename_all = "camelCase")]
pub struct ScoutConfig {
    pub interests: super::workflow_scout::InterestsConfig,
    pub user_context: super::workflow_scout::UserContextConfig,
}

// ---------------------------------------------------------------------------
// Channels
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelsConfig {
    pub telegram: TelegramConfig,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
#[serde(rename_all = "camelCase")]
pub struct GatewayConfig {
    pub dashboard: DashboardConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
#[serde(rename_all = "camelCase")]
pub struct CaptainConfig {
    pub auto_schedule: bool,
    pub auto_merge: bool,
    pub max_concurrent_workers: Option<usize>,
    pub tick_interval_s: u64,
    pub tz: String,
    pub default_terminal_agent: TerminalAgent,
    /// Extra CLI arguments appended when spawning Claude Code terminals.
    pub claude_terminal_args: String,
    /// Extra CLI arguments appended when spawning Codex terminals.
    pub codex_terminal_args: String,
    #[serde(skip)]
    pub projects: HashMap<String, ProjectConfig>,
    #[serde(skip)]
    pub task_db_path: String,
    #[serde(skip)]
    pub lockfile_path: String,
    #[serde(skip)]
    pub worker_health_path: String,
}

fn default_task_db_path() -> String {
    global_infra::paths::data_dir()
        .join("mando.db")
        .to_string_lossy()
        .into_owned()
}

fn default_lockfile_path() -> String {
    global_infra::paths::data_dir()
        .join("captain.lock")
        .to_string_lossy()
        .into_owned()
}

fn default_worker_health_path() -> String {
    global_infra::paths::state_dir()
        .join("worker-health.json")
        .to_string_lossy()
        .into_owned()
}

impl Default for CaptainConfig {
    fn default() -> Self {
        Self {
            auto_schedule: false,
            auto_merge: false,
            max_concurrent_workers: None,
            tick_interval_s: 30,
            tz: iana_time_zone::get_timezone().unwrap_or_else(|_| "UTC".into()),
            default_terminal_agent: TerminalAgent::Claude,
            claude_terminal_args: "--dangerously-skip-permissions".into(),
            codex_terminal_args: "--full-auto".into(),
            projects: HashMap::new(),
            task_db_path: default_task_db_path(),
            lockfile_path: default_lockfile_path(),
            worker_health_path: default_worker_health_path(),
        }
    }
}

impl CaptainConfig {
    fn populate_runtime_paths(&mut self) {
        self.task_db_path = default_task_db_path();
        self.lockfile_path = default_lockfile_path();
        self.worker_health_path = default_worker_health_path();
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
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
