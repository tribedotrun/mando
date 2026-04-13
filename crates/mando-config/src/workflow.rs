//! Workflow configuration — loads workflow.yaml, renders prompt templates.
//!
//! Two workflow files:
//! - **Captain** (`captain/workflow.yaml`): orchestration, prompts, nudges
//! - **Scout** (`scout/workflow.yaml`): triage, article, research, QA
//!
//! Binary ships a compiled-in default; user can override at `~/.mando/workflow.yaml`
//! (captain) or `~/.mando/scout-workflow.yaml` (scout).

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

// Template rendering lives in workflow_render.rs; re-export public API.
pub use super::workflow_render::{
    render_initial_prompt, render_nudge, render_prompt, render_template, validate_template_syntax,
};

use crate::error::ConfigError;

// ── Types ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CaptainWorkflow {
    pub models: ModelsConfig,
    pub agent: AgentConfig,
    pub auto_title: AutoTitleConfig,
    pub prompts: HashMap<String, String>,
    pub nudges: HashMap<String, String>,
    pub initial_prompts: HashMap<String, String>,
}

impl CaptainWorkflow {
    /// Load from compiled-in default YAML.
    /// Panics if the compiled-in asset is malformed — that is a build defect.
    pub fn compiled_default() -> Self {
        serde_yaml::from_str(DEFAULT_CAPTAIN_WORKFLOW)
            .expect("compiled captain-workflow.yaml is malformed — this is a build defect")
    }
}

pub use super::workflow_scout::{
    InterestsConfig, ScoutAgentConfig, ScoutRepo, ScoutWorkflow, UserContextConfig,
};

fn default_fallback_model() -> Option<String> {
    Some("sonnet[1m]".into())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ModelsConfig {
    pub worker: String,
    pub captain: String,
    pub clarifier: String,
    pub todo_parse: String,
    /// Fallback model when primary hits rate limits (e.g. "sonnet" as fallback for "opus").
    #[serde(default = "default_fallback_model")]
    pub fallback: Option<String>,
}

impl Default for ModelsConfig {
    fn default() -> Self {
        Self {
            worker: "default".into(),
            captain: "default".into(),
            clarifier: "default".into(),
            todo_parse: "default".into(),
            fallback: Some("sonnet[1m]".into()),
        }
    }
}

/// Configuration for auto-generating terminal workbench titles.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AutoTitleConfig {
    pub model: String,
    pub prompt: String,
    /// Timeout for the `claude -p` subprocess.
    #[serde(with = "duration_seconds")]
    pub timeout_s: std::time::Duration,
    /// How often the background loop checks for pending titles.
    #[serde(with = "duration_seconds")]
    pub poll_interval_s: std::time::Duration,
    /// Give up if the workbench is older than this.
    #[serde(with = "duration_seconds")]
    pub expiry_s: std::time::Duration,
    /// Truncate the user's first message to this many characters.
    pub max_input_chars: usize,
}

impl Default for AutoTitleConfig {
    fn default() -> Self {
        Self {
            model: "haiku".into(),
            prompt: "Generate a concise 3-5 word title for this conversation. \
                     Output ONLY the title, nothing else:"
                .into(),
            timeout_s: std::time::Duration::from_secs(30),
            poll_interval_s: std::time::Duration::from_secs(60),
            expiry_s: std::time::Duration::from_secs(300),
            max_input_chars: 200,
        }
    }
}

/// Serde adapter that reads/writes a `Duration` as a floating-point seconds value.
/// Used for every timeout/delay field in `AgentConfig` so the wire format stays
/// a plain number in workflow.yaml while the in-memory type enforces unit safety.
mod duration_seconds {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::Duration;

    pub fn serialize<S: Serializer>(d: &Duration, s: S) -> Result<S::Ok, S::Error> {
        d.as_secs_f64().serialize(s)
    }
    /// Upper bound on deserialized durations. `Duration::from_secs_f64` panics
    /// when the value overflows the internal representation (roughly
    /// `u64::MAX` seconds), so we need a hard cap here. 1e18 seconds is about
    /// 31 billion years, which is far beyond any legitimate timeout/delay and
    /// leaves plenty of headroom below the actual overflow boundary.
    const MAX_DURATION_SECS: f64 = 1e18;

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Duration, D::Error> {
        let secs = f64::deserialize(d)?;
        // Reject non-finite values (NaN, +inf, -inf) and values large enough
        // to overflow `Duration::from_secs_f64`. YAML 1.1 accepts `.inf` and
        // `.nan` as valid floats, and `from_secs_f64` panics on both those
        // and any finite value above ~u64::MAX seconds. Catching the panic
        // inside try_load_captain_workflow is not enough because some call
        // sites deserialize AgentConfig outside the catch_unwind wrapper.
        if !secs.is_finite() || secs > MAX_DURATION_SECS {
            return Err(serde::de::Error::custom(format!(
                "duration must be finite and <= {MAX_DURATION_SECS:e} seconds, got {secs}"
            )));
        }
        Ok(Duration::from_secs_f64(secs.max(0.0)))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentConfig {
    pub max_concurrent: usize,
    pub resource_limits: HashMap<String, usize>,
    pub max_interventions: u32,
    /// Seconds a stream can go without activity before workers are nudged.
    #[serde(with = "duration_seconds")]
    pub stale_threshold_s: std::time::Duration,
    #[serde(with = "duration_seconds")]
    pub captain_review_timeout_s: std::time::Duration,
    #[serde(with = "duration_seconds")]
    pub captain_merge_timeout_s: std::time::Duration,
    pub max_review_retries: u32,
    pub max_merge_retries: u32,
    pub max_clarifier_retries: u32,
    pub max_rebase_retries: u32,
    #[serde(with = "duration_seconds")]
    pub rebase_base_delay_s: std::time::Duration,
    #[serde(with = "duration_seconds")]
    pub worker_timeout_s: std::time::Duration,
    #[serde(with = "duration_seconds")]
    pub clarifier_timeout_s: std::time::Duration,
    #[serde(with = "duration_seconds")]
    pub todo_parse_timeout_s: std::time::Duration,
    #[serde(with = "duration_seconds")]
    pub todo_parse_idle_ttl_s: std::time::Duration,
    pub todo_parse_max_turns: u32,
    /// How long an item can sit in NeedsClarification (waiting for human) before
    /// escalating to CaptainReviewing. Much larger than clarifier_timeout_s
    /// because humans respond in hours/days, not seconds.
    #[serde(with = "duration_seconds")]
    pub needs_clarification_timeout_s: std::time::Duration,
    #[serde(with = "duration_seconds")]
    pub archive_grace_secs: std::time::Duration,
    #[serde(with = "duration_seconds")]
    pub evidence_download_timeout_s: std::time::Duration,
    #[serde(with = "duration_seconds")]
    pub evidence_ffmpeg_timeout_s: std::time::Duration,
    /// Circuit breaker: route to captain review after this many consecutive
    /// nudges with the identical reason.
    pub max_repeated_nudges: u32,
    /// Timeout for task-ask CC sessions (user Q&A with worktree access).
    #[serde(with = "duration_seconds")]
    pub task_ask_timeout_s: std::time::Duration,
    /// Idle TTL for task-ask CC sessions before they are reaped.
    #[serde(with = "duration_seconds")]
    pub task_ask_idle_ttl_s: std::time::Duration,
    /// Timeout for ops CC sessions (generic ephemeral sessions).
    #[serde(with = "duration_seconds")]
    pub ops_timeout_s: std::time::Duration,
    /// Idle TTL for ops CC sessions.
    #[serde(with = "duration_seconds")]
    pub ops_idle_ttl_s: std::time::Duration,
}

impl Default for AgentConfig {
    fn default() -> Self {
        use std::time::Duration;
        Self {
            max_concurrent: 10,
            resource_limits: HashMap::new(),
            max_interventions: 50,
            stale_threshold_s: Duration::from_secs(1200),
            captain_review_timeout_s: Duration::from_secs(1200),
            captain_merge_timeout_s: Duration::from_secs(1800),
            max_review_retries: 5,
            max_merge_retries: 3,
            max_clarifier_retries: 3,
            max_rebase_retries: 5,
            rebase_base_delay_s: Duration::from_secs(30),
            worker_timeout_s: Duration::from_secs(21600),
            clarifier_timeout_s: Duration::from_secs(1800),
            todo_parse_timeout_s: Duration::from_secs(300),
            todo_parse_idle_ttl_s: Duration::from_secs(120),
            todo_parse_max_turns: 10,
            needs_clarification_timeout_s: Duration::from_secs(86400), // 24 hours
            archive_grace_secs: Duration::from_secs(604800),
            evidence_download_timeout_s: Duration::from_secs(30),
            evidence_ffmpeg_timeout_s: Duration::from_secs(30),
            max_repeated_nudges: 3,
            task_ask_timeout_s: Duration::from_secs(600),
            task_ask_idle_ttl_s: Duration::from_secs(3600),
            ops_timeout_s: Duration::from_secs(120),
            ops_idle_ttl_s: Duration::from_secs(3600),
        }
    }
}

// ── Embedded defaults ────────────────────────────────────────────────────────

const DEFAULT_CAPTAIN_WORKFLOW: &str = include_str!("../assets/captain-workflow.yaml");

// ── Loading ──────────────────────────────────────────────────────────────────

fn parse_captain_workflow(yaml: &str, path: &Path) -> Result<CaptainWorkflow, ConfigError> {
    serde_yaml::from_str(yaml).map_err(|e| ConfigError::YamlParse {
        path: path.to_path_buf(),
        source: e,
    })
}

fn parse_scout_workflow(yaml: &str, path: &Path) -> Result<ScoutWorkflow, ConfigError> {
    serde_yaml::from_str(yaml).map_err(|e| ConfigError::YamlParse {
        path: path.to_path_buf(),
        source: e,
    })
}

/// Load captain workflow: user override at `path` if it exists, else compiled-in default.
/// `tick_interval_s` comes from `CaptainConfig` for timing-invariant validation.
/// Returns an error if the user override YAML fails to parse. Panics if required
/// template keys are missing or agent config is invalid (fail-fast at startup).
pub fn load_captain_workflow(
    override_path: &Path,
    tick_interval_s: u64,
) -> Result<CaptainWorkflow, ConfigError> {
    let wf = load_captain_workflow_inner(override_path)?;
    crate::workflow_validate::validate_captain_workflow(&wf);
    crate::workflow_validate::validate_agent_config(&wf.agent, tick_interval_s);
    Ok(wf)
}

/// Non-panicking variant for use in HTTP handlers.
/// Returns a typed error if parsing, template validation, or agent config
/// validation fails.
pub fn try_load_captain_workflow(
    override_path: &Path,
    tick_interval_s: u64,
) -> Result<CaptainWorkflow, ConfigError> {
    let wf = load_captain_workflow_inner(override_path)?;
    // validate_captain_workflow panics on missing template keys — catch it and
    // surface as a ConfigError::Validation.
    if let Err(e) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        crate::workflow_validate::validate_captain_workflow(&wf);
    })) {
        let msg = e
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| e.downcast_ref::<&str>().map(|s| (*s).to_string()))
            .unwrap_or_else(|| "workflow validation failed".to_string());
        return Err(ConfigError::Validation(msg));
    }
    crate::workflow_validate::try_validate_agent_config(&wf.agent, tick_interval_s)
        .map_err(ConfigError::Validation)?;
    Ok(wf)
}

fn load_captain_workflow_inner(override_path: &Path) -> Result<CaptainWorkflow, ConfigError> {
    if override_path.exists() {
        let contents = std::fs::read_to_string(override_path).map_err(|e| ConfigError::Io {
            op: "read".into(),
            path: override_path.to_path_buf(),
            source: e,
        })?;
        tracing::info!("loaded captain workflow from {}", override_path.display());
        parse_captain_workflow(&contents, override_path)
    } else {
        Ok(CaptainWorkflow::compiled_default())
    }
}

/// Load scout workflow: user override at `path` if it exists, else compiled-in default.
/// Repos are injected from `config` projects that have a non-empty `scout_summary`.
/// Returns an error if the user override YAML fails to parse. Panics if required
/// template keys are missing (fail-fast at startup).
pub fn load_scout_workflow(
    override_path: &Path,
    config: &crate::Config,
) -> Result<ScoutWorkflow, ConfigError> {
    let mut wf = if override_path.exists() {
        let contents = std::fs::read_to_string(override_path).map_err(|e| ConfigError::Io {
            op: "read".into(),
            path: override_path.to_path_buf(),
            source: e,
        })?;
        tracing::info!("loaded scout workflow from {}", override_path.display());
        parse_scout_workflow(&contents, override_path)?
    } else {
        ScoutWorkflow::compiled_default()
    };
    inject_config_into_workflow(&mut wf, config);
    crate::workflow_validate::validate_scout_workflow(&wf);
    Ok(wf)
}

/// Merge per-user scout settings from config.json into the workflow.
/// config.json is the canonical source for interests and user_context —
/// always overrides whatever the yaml had.
fn inject_config_into_workflow(wf: &mut ScoutWorkflow, config: &crate::Config) {
    let dc = &config.scout;

    wf.interests = dc.interests.clone();
    wf.user_context = dc.user_context.clone();

    // Repos: merge from projects that have a scout_summary.
    let existing_names: std::collections::HashSet<String> =
        wf.repos.iter().map(|r| r.name.to_lowercase()).collect();

    for project in config.captain.projects.values() {
        if project.scout_summary.is_empty() {
            continue;
        }
        if existing_names.contains(&project.name.to_lowercase()) {
            continue;
        }
        wf.repos.push(super::workflow_scout::ScoutRepo {
            name: project.name.clone(),
            path: project.path.clone(),
            summary: project.scout_summary.clone(),
        });
    }
}

// ── Path helpers ─────────────────────────────────────────────────────────────

/// Default path for captain workflow override: `~/.mando/workflow.yaml`.
pub fn captain_workflow_path() -> std::path::PathBuf {
    crate::paths::data_dir().join("workflow.yaml")
}

/// Default path for scout workflow override: `~/.mando/scout-workflow.yaml`.
pub fn scout_workflow_path() -> std::path::PathBuf {
    crate::paths::data_dir().join("scout-workflow.yaml")
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustc_hash::FxHashMap;

    #[test]
    fn load_default_captain_workflow() {
        let wf = CaptainWorkflow::compiled_default();
        assert!(!wf.prompts.is_empty(), "should have prompts");
        assert!(!wf.nudges.is_empty(), "should have nudges");
        assert!(wf.agent.max_interventions > 0);
        assert!(wf.agent.max_review_retries > 0);
    }

    #[test]
    fn load_default_scout_workflow() {
        let wf = ScoutWorkflow::compiled_default();
        assert!(!wf.prompts.is_empty(), "should have prompts");
        assert!(wf.interests.high.is_empty());
        assert!(wf.user_context.role.is_empty());
    }

    #[test]
    fn scout_config_overrides_workflow() {
        let mut config = crate::Config::default();
        config.scout.interests.high = vec!["Custom interest".into()];
        config.scout.user_context.role = "Custom role".into();
        let mut wf = ScoutWorkflow::compiled_default();
        super::inject_config_into_workflow(&mut wf, &config);
        assert_eq!(wf.interests.high, vec!["Custom interest"]);
        assert_eq!(wf.user_context.role, "Custom role");
    }

    #[test]
    fn render_worker_initial_prompt() {
        let wf = CaptainWorkflow::compiled_default();
        let mut vars: FxHashMap<&str, &str> = FxHashMap::default();
        vars.insert("title", "Fix the login bug");
        vars.insert("context", "Auth module is broken");
        vars.insert("branch", "mando/fix-login-1");
        vars.insert("id", "42");
        vars.insert("no_pr", "false");
        vars.insert("original_prompt", "");
        vars.insert("worker_preamble", "");
        vars.insert("check_command", "`mando-dev check`");
        vars.insert("workpad_path", "/tmp/mando/plans/42/workpad.md");

        let result = render_prompt("worker_initial", &wf.prompts, &vars);
        assert!(result.is_ok());
        let rendered = result.unwrap();
        assert!(rendered.contains("Fix the login bug"));
        assert!(rendered.contains("mando/fix-login-1"));
        assert!(rendered.contains("/tmp/mando/plans/42/workpad.md"));
    }

    #[test]
    fn render_initial_prompt_template() {
        let wf = CaptainWorkflow::compiled_default();
        let mut vars: FxHashMap<&str, &str> = FxHashMap::default();
        vars.insert("brief_filename", "brief.md");
        vars.insert("brief_path", "/tmp/mando/.ai/briefs/brief.md");
        vars.insert("id", "42");
        vars.insert("no_pr", "false");
        vars.insert("workpad_path", "/tmp/mando/plans/42/workpad.md");

        let rendered = render_initial_prompt("worker", &wf.initial_prompts, &vars).unwrap();
        assert!(rendered.contains("/tmp/mando/.ai/briefs/brief.md"));
        assert!(rendered.contains("42"));
        assert!(rendered.contains("Before you update the workpad, first read"));
        assert!(rendered.contains("/tmp/mando/plans/42/workpad.md"));
    }

    #[test]
    fn render_rebase_worker_retry_guidance() {
        let wf = CaptainWorkflow::compiled_default();
        let mut vars: FxHashMap<&str, &str> = FxHashMap::default();
        vars.insert("branch", "mando/test");
        vars.insert("default_branch", "main");
        vars.insert("pr_num", "311");
        vars.insert("attempt", "2");
        vars.insert("max_retries", "5");

        let rendered = render_prompt("rebase_worker", &wf.prompts, &vars).unwrap();
        assert!(rendered.contains("attempt 2/5"));
        assert!(rendered.contains("prior rebase attempt failed"));
    }

    #[test]
    fn render_captain_merge_prompt_for_blocking_ci() {
        let wf = CaptainWorkflow::compiled_default();
        let mut vars: FxHashMap<&str, &str> = FxHashMap::default();
        vars.insert("pr_url", "https://github.com/tribedotrun/mando/pull/334");
        vars.insert("repo", "tribedotrun/mando");
        vars.insert("pr_number", "334");
        vars.insert("title", "Remove Triage all pending button from Scout page");

        let rendered = render_prompt("captain_merge", &wf.prompts, &vars).unwrap();

        assert!(rendered.contains("gh pr checks 334 --repo tribedotrun/mando --required"));
        assert!(rendered.contains("--required --watch --fail-fast"));
        assert!(rendered.contains("15 minutes"));
        assert!(rendered.contains("gh pr merge 334 --repo tribedotrun/mando --squash"));
        assert!(rendered.contains("gh pr merge 334 --repo tribedotrun/mando --squash --admin"));
        assert!(rendered.contains("Do not use `/ci`"));

        assert!(!rendered.contains("Read `.github/workflows/` to understand how CI is triggered"));
        assert!(!rendered.contains("Take the appropriate action to trigger CI"));
        assert!(!rendered.contains("every 30 seconds"));
    }

    #[test]
    fn validate_compiled_captain_workflow() {
        let wf = CaptainWorkflow::compiled_default();
        crate::workflow_validate::validate_captain_workflow(&wf);
    }

    #[test]
    fn validate_compiled_scout_workflow() {
        let wf = ScoutWorkflow::compiled_default();
        crate::workflow_validate::validate_scout_workflow(&wf);
    }

    #[test]
    #[should_panic(expected = "captain workflow missing required template keys")]
    fn validate_captain_missing_key_panics() {
        let mut wf = CaptainWorkflow::compiled_default();
        wf.prompts.remove("worker_initial");
        crate::workflow_validate::validate_captain_workflow(&wf);
    }

    #[test]
    #[should_panic(expected = "scout workflow missing required template keys")]
    fn validate_scout_missing_key_panics() {
        let mut wf = ScoutWorkflow::compiled_default();
        wf.prompts.remove("process");
        crate::workflow_validate::validate_scout_workflow(&wf);
    }

    #[test]
    #[should_panic(expected = "missing:")]
    fn validate_reports_missing_and_syntax_together() {
        let mut wf = CaptainWorkflow::compiled_default();
        wf.prompts.remove("worker_initial");
        wf.prompts
            .insert("worker_briefed".into(), "{% if unclosed %}".into());
        crate::workflow_validate::validate_captain_workflow(&wf);
    }
}
