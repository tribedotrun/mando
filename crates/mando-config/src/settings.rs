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
    pub voice: VoiceConfig,
    pub scout: ScoutConfig,
    pub tools: ToolsConfig,
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
            voice: VoiceConfig::default(),
            scout: ScoutConfig::default(),
            tools: ToolsConfig::default(),
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
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FeaturesConfig {
    pub voice: bool,
    pub decision_journal: bool,
    pub cron: bool,
    pub linear: bool,
    pub dev_mode: bool,
    pub analytics: bool,
    pub setup_dismissed: bool,
    pub claude_code_verified: bool,
}

impl Default for FeaturesConfig {
    fn default() -> Self {
        Self {
            voice: false,
            decision_journal: false,
            cron: true,
            linear: false,
            dev_mode: false,
            analytics: false,
            setup_dismissed: false,
            claude_code_verified: false,
        }
    }
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
// Voice (settings only — gated by features.voice)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct VoiceConfig {
    pub voice_id: String,
    pub model: String,
    pub usage_warning_threshold: f64,
    pub session_expiry_days: u32,
}

impl Default for VoiceConfig {
    fn default() -> Self {
        Self {
            voice_id: "EXAVITQu4vr4xnSDxMaL".into(),
            model: "eleven_flash_v2_5".into(),
            usage_warning_threshold: 0.8,
            session_expiry_days: 7,
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
    pub learn_cron_expr: String,
    pub tz: String,
    pub projects: HashMap<String, ProjectConfig>,
    #[serde(skip)]
    pub task_db_path: String,
    #[serde(skip)]
    pub lockfile_path: String,
    #[serde(skip)]
    pub worker_health_path: String,
    pub linear_team: String,
    pub linear_cli_path: String,
}

impl Default for CaptainConfig {
    fn default() -> Self {
        Self {
            auto_schedule: false,
            tick_interval_s: 30,
            learn_cron_expr: "0 9 * * *".into(),
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
            linear_team: String::new(),
            linear_cli_path: String::new(),
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
// Tools
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ToolsConfig {
    #[serde(skip_serializing)]
    pub cc_self_improve: CCSelfImproveConfig,
}

// ---------------------------------------------------------------------------
// CC Self-Improve (gated by features.devMode)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CCSelfImproveConfig {
    pub monitor_logs: bool,
    pub monitor_outbound: bool,
    pub log_paths: Vec<String>,
    pub error_patterns: Vec<String>,
    pub ignore_patterns: Vec<String>,
    pub poll_interval_s: f64,
    pub cooldown_s: u64,
    pub max_repairs_per_hour: u32,
    pub timeout_s: u64,
    pub model: String,
    pub cwd: String,
}

impl Default for CCSelfImproveConfig {
    fn default() -> Self {
        Self {
            monitor_logs: true,
            monitor_outbound: true,
            log_paths: vec![
                "~/.mando/logs/gateway.stderr.log".into(),
                "~/.mando/logs/gateway.stdout.log".into(),
            ],
            error_patterns: vec![
                r"\[captain\].*planner spawn failed".into(),
                r"directory already exists".into(),
                r"could not parse clarifier output".into(),
                r"\| CRITICAL\b".into(),
                r"\| ERROR\b".into(),
            ],
            ignore_patterns: vec![
                r"exception happened while polling for updates".into(),
                r"nodename nor servname provided, or not known".into(),
                r"telegram\.error\.(NetworkError|TimedOut|RetryAfter|Forbidden|BadRequest)".into(),
                r"httpx\.(ReadError|ConnectError|RemoteProtocolError|TimeoutException|ConnectTimeout)".into(),
                r"httpcore\.(ReadError|ConnectError|RemoteProtocolError)".into(),
                r"\[captain\] (action|deterministic):".into(),
                r"\[spawner\] nudged".into(),
                r"\[self-improve\]".into(),
            ],
            poll_interval_s: 2.0,
            cooldown_s: 300,
            max_repairs_per_hour: 3,
            timeout_s: 900,
            model: "default".into(),
            cwd: ".".into(),
        }
    }
}

// ---------------------------------------------------------------------------
// CC Worktree Cleanup
// ---------------------------------------------------------------------------
