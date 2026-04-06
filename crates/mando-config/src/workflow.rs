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
use std::sync::{LazyLock, RwLock};

use minijinja::Environment;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use serde_json::{Map as JsonMap, Number as JsonNumber, Value as JsonValue};

use crate::error::ConfigError;

// ── Types ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct CaptainWorkflow {
    pub models: ModelsConfig,
    pub agent: AgentConfig,
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

pub use super::workflow_scout::{InterestsConfig, ScoutRepo, ScoutWorkflow, UserContextConfig};

fn default_fallback_model() -> Option<String> {
    Some("sonnet[1m]".into())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ModelsConfig {
    pub worker: String,
    pub captain: String,
    pub clarifier: String,
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
            fallback: Some("sonnet[1m]".into()),
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
    pub max_clarifier_retries: u32,
    pub max_rebase_retries: u32,
    #[serde(with = "duration_seconds")]
    pub rebase_base_delay_s: std::time::Duration,
    #[serde(with = "duration_seconds")]
    pub worker_timeout_s: std::time::Duration,
    #[serde(with = "duration_seconds")]
    pub clarifier_timeout_s: std::time::Duration,
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
            max_clarifier_retries: 3,
            max_rebase_retries: 5,
            rebase_base_delay_s: Duration::from_secs(30),
            worker_timeout_s: Duration::from_secs(21600),
            clarifier_timeout_s: Duration::from_secs(1800),
            needs_clarification_timeout_s: Duration::from_secs(86400), // 24 hours
            archive_grace_secs: Duration::from_secs(604800),
            evidence_download_timeout_s: Duration::from_secs(30),
            evidence_ffmpeg_timeout_s: Duration::from_secs(30),
            max_repeated_nudges: 3,
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

// ── Template rendering ───────────────────────────────────────────────────────

/// Render a template string with MiniJinja.
///
/// `vars` is an `FxHashMap` (rustc-hash) — the template-var hot path uses this
/// faster (but HashDoS-vulnerable) hasher because the keys are hard-coded
/// compile-time literals, not untrusted input.
pub fn render_template<V: AsRef<str>>(
    template: &str,
    vars: &FxHashMap<&str, V>,
) -> Result<String, String> {
    render_template_value_map(template, &coerce_template_vars(vars))
}

/// Signature of a template map used to detect hot-reloads.
///
/// We can't hash `&HashMap<String, String>` directly because `HashMap` has a
/// non-deterministic iteration order. We build a sorted `(name, content)` vec
/// under a stable hasher to get a stable fingerprint.
fn template_map_signature(templates: &HashMap<String, String>) -> u64 {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut entries: Vec<(&String, &String)> = templates.iter().collect();
    entries.sort_by(|a, b| a.0.cmp(b.0));
    let mut h = DefaultHasher::new();
    for (k, v) in entries {
        k.hash(&mut h);
        v.hash(&mut h);
    }
    h.finish()
}

/// Cached MiniJinja environment keyed by the current template-map signature.
///
/// Building an `Environment` and re-adding every template on each render is
/// expensive. We cache compiled `Environment<'static>` values keyed by the
/// content signature of the template map that built them.
///
/// Multi-slot: captain ticks alternate between nudge renders (classify phase)
/// and prompt renders (spawn/review phase) in the same tick. A single-slot
/// cache would thrash every phase transition because nudges and prompts are
/// separate maps with different signatures. Keying on signature and keeping
/// both maps resident (small HashMap, typically at most 4 entries: captain
/// nudges, captain prompts, scout nudges, scout prompts) gives cache hits for
/// both phases. Entries are only invalidated on hot-reload when the workflow
/// file changes, which produces a new signature and leaves the old entry
/// unreachable (still pinned in the map until next clear). An LRU bound
/// protects against unbounded growth in pathological workloads where the
/// template map changes per render.
struct CachedEnv {
    env: Environment<'static>,
}

const RENDER_ENV_CACHE_MAX: usize = 8;

static RENDER_ENV_CACHE: LazyLock<RwLock<FxHashMap<u64, CachedEnv>>> =
    LazyLock::new(|| RwLock::new(FxHashMap::default()));

/// Look up a named template from a map and render it with the given variables.
/// All templates from the map are registered in the environment so that
/// `{% include "other_template" %}` works across entries.
fn render_named<V: AsRef<str>>(
    kind: &str,
    template_name: &str,
    templates: &HashMap<String, String>,
    vars: &FxHashMap<&str, V>,
) -> Result<String, String> {
    if !templates.contains_key(template_name) {
        return Err(format!("unknown {kind} template: {template_name:?}"));
    }
    let signature = template_map_signature(templates);

    // Fast path: read-lock and render from the cached environment for this
    // specific template map's signature.
    {
        let guard = RENDER_ENV_CACHE.read().map_err(|e| e.to_string())?;
        if let Some(cached) = guard.get(&signature) {
            let tmpl = cached
                .env
                .get_template(template_name)
                .map_err(|e| e.to_string())?;
            return tmpl
                .render(JsonValue::Object(coerce_template_vars(vars)))
                .map_err(|e| e.to_string());
        }
    }

    // Slow path: rebuild the environment. Build fresh strings to own them as 'static.
    let mut env: Environment<'static> = Environment::new();
    for (name, content) in templates {
        env.add_template_owned(name.clone(), content.clone())
            .map_err(|e| e.to_string())?;
    }

    let result = {
        let tmpl = env.get_template(template_name).map_err(|e| e.to_string())?;
        tmpl.render(JsonValue::Object(coerce_template_vars(vars)))
            .map_err(|e| e.to_string())?
    };

    // Store rebuilt env for the next renders, keyed by its signature. If the
    // cache has grown past the soft bound, drop the oldest-accessed entry via
    // simple eviction (clear all and re-insert, since captain workflows
    // typically only need 4 keys at steady state).
    if let Ok(mut guard) = RENDER_ENV_CACHE.write() {
        if guard.len() >= RENDER_ENV_CACHE_MAX && !guard.contains_key(&signature) {
            guard.clear();
        }
        guard.insert(signature, CachedEnv { env });
    }

    Ok(result)
}

/// Render a named prompt from a workflow's prompt map.
pub fn render_prompt<V: AsRef<str>>(
    template_name: &str,
    prompts: &HashMap<String, String>,
    vars: &FxHashMap<&str, V>,
) -> Result<String, String> {
    render_named("prompt", template_name, prompts, vars)
}

/// Render a named nudge from a workflow's nudge map.
pub fn render_nudge<V: AsRef<str>>(
    template_name: &str,
    nudges: &HashMap<String, String>,
    vars: &FxHashMap<&str, V>,
) -> Result<String, String> {
    Ok(render_named("nudge", template_name, nudges, vars)?
        .trim()
        .to_string())
}

/// Render a named initial prompt from a workflow's initial-prompt map.
pub fn render_initial_prompt<V: AsRef<str>>(
    template_name: &str,
    prompts: &HashMap<String, String>,
    vars: &FxHashMap<&str, V>,
) -> Result<String, String> {
    render_named("initial prompt", template_name, prompts, vars)
}

pub fn validate_template_syntax(template: &str) -> Result<(), String> {
    let mut env = Environment::new();
    env.add_template("template", template)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

fn render_template_value_map(
    template: &str,
    vars: &JsonMap<String, JsonValue>,
) -> Result<String, String> {
    let mut env = Environment::new();
    env.add_template("template", template)
        .map_err(|e| e.to_string())?;
    let tmpl = env.get_template("template").map_err(|e| e.to_string())?;
    tmpl.render(JsonValue::Object(vars.clone()))
        .map_err(|e| e.to_string())
}

fn coerce_template_vars<V: AsRef<str>>(vars: &FxHashMap<&str, V>) -> JsonMap<String, JsonValue> {
    vars.iter()
        .map(|(key, value)| ((*key).to_string(), coerce_template_scalar(value.as_ref())))
        .collect()
}

fn coerce_template_scalar(value: &str) -> JsonValue {
    match value {
        "true" => JsonValue::Bool(true),
        "false" => JsonValue::Bool(false),
        _ => try_parse_integer(value)
            .map(|n| JsonValue::Number(JsonNumber::from(n)))
            .unwrap_or_else(|| JsonValue::String(value.to_string())),
    }
}

fn try_parse_integer(value: &str) -> Option<i64> {
    if value.is_empty() {
        return None;
    }
    let trimmed = value.strip_prefix('-').unwrap_or(value);
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.starts_with('0') && trimmed.len() > 1 {
        return None;
    }
    if !trimmed.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    value.parse::<i64>().ok()
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

    #[test]
    fn render_simple_variable() {
        let mut vars: FxHashMap<&str, &str> = FxHashMap::default();
        vars.insert("name", "world");
        assert_eq!(
            render_template("Hello {{ name }}!", &vars).unwrap(),
            "Hello world!"
        );
    }

    #[test]
    fn render_if_true() {
        let mut vars: FxHashMap<&str, &str> = FxHashMap::default();
        vars.insert("show", "yes");
        let result = render_template("{% if show %}visible{% endif %}", &vars).unwrap();
        assert_eq!(result, "visible");
    }

    #[test]
    fn render_if_false() {
        let vars: FxHashMap<&str, &str> = FxHashMap::default();
        let result = render_template("{% if show %}visible{% endif %}", &vars).unwrap();
        assert_eq!(result, "");
    }

    #[test]
    fn render_if_not() {
        let vars: FxHashMap<&str, &str> = FxHashMap::default();
        let result = render_template("{% if not show %}fallback{% endif %}", &vars).unwrap();
        assert_eq!(result, "fallback");
    }

    #[test]
    fn render_nested_if() {
        let mut vars: FxHashMap<&str, &str> = FxHashMap::default();
        vars.insert("a", "1");
        vars.insert("b", "2");
        let tmpl = "{% if a %}A{% if b %}B{% endif %}{% endif %}";
        assert_eq!(render_template(tmpl, &vars).unwrap(), "AB");
    }

    #[test]
    fn render_inline_if_expression() {
        let mut vars: FxHashMap<&str, &str> = FxHashMap::default();
        vars.insert("id", "42");
        let tmpl = "{{ '4' if id else '3' }}";
        assert_eq!(render_template(tmpl, &vars).unwrap(), "4");
    }

    #[test]
    fn render_numeric_comparison() {
        let mut vars: FxHashMap<&str, &str> = FxHashMap::default();
        vars.insert("attempt", "2");
        let tmpl = "{% if attempt > 1 %}retry{% endif %}";
        assert_eq!(render_template(tmpl, &vars).unwrap(), "retry");
    }

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
        // interests/user_context default to empty — user fills them in via settings.
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

        let result = render_prompt("worker_initial", &wf.prompts, &vars);
        assert!(result.is_ok());
        let rendered = result.unwrap();
        assert!(rendered.contains("Fix the login bug"));
        assert!(rendered.contains("mando/fix-login-1"));
    }

    #[test]
    fn render_initial_prompt_template() {
        let wf = CaptainWorkflow::compiled_default();
        let mut vars: FxHashMap<&str, &str> = FxHashMap::default();
        vars.insert("brief_filename", "brief.md");
        vars.insert("id", "42");
        vars.insert("no_pr", "false");

        let rendered = render_initial_prompt("worker", &wf.initial_prompts, &vars).unwrap();
        assert!(rendered.contains(".ai/briefs/brief.md"));
        assert!(rendered.contains("42"));
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
        println!("{rendered}");
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
    fn template_values_not_reparsed() {
        // Values containing {{ are NOT re-parsed as template syntax because
        // they go directly into the output buffer, not back into remaining.
        let mut vars: FxHashMap<&str, &str> = FxHashMap::default();
        vars.insert("content", "Use {{ env.VAR }} here");
        let result = render_template("Content: {{ content }}", &vars).unwrap();
        assert_eq!(result, "Content: Use {{ env.VAR }} here");
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

    // ── coerce_template_scalar edge cases ───────────────────────────

    #[test]
    fn coerce_true_false_to_bool() {
        assert_eq!(coerce_template_scalar("true"), JsonValue::Bool(true));
        assert_eq!(coerce_template_scalar("false"), JsonValue::Bool(false));
    }

    #[test]
    fn coerce_case_sensitive_not_bool() {
        // "TRUE" and "False" should stay as strings — only exact lowercase matches.
        assert!(coerce_template_scalar("TRUE").is_string());
        assert!(coerce_template_scalar("False").is_string());
    }

    #[test]
    fn coerce_integers() {
        assert_eq!(coerce_template_scalar("0"), JsonValue::Number(0.into()));
        assert_eq!(coerce_template_scalar("42"), JsonValue::Number(42.into()));
        assert_eq!(coerce_template_scalar("-1"), JsonValue::Number((-1).into()));
    }

    #[test]
    fn coerce_leading_zero_stays_string() {
        // "00123" should NOT be parsed as integer 123 — it's a string (e.g. a code).
        assert!(coerce_template_scalar("00123").is_string());
        assert!(coerce_template_scalar("007").is_string());
    }

    #[test]
    fn coerce_empty_string_stays_string() {
        assert!(coerce_template_scalar("").is_string());
    }

    #[test]
    fn coerce_plain_text_stays_string() {
        assert!(coerce_template_scalar("hello").is_string());
        assert!(coerce_template_scalar("ENG-42").is_string());
    }

    // ── render_template error contract ──────────────────────────────

    #[test]
    fn render_template_returns_err_on_bad_syntax() {
        let vars: FxHashMap<&str, &str> = FxHashMap::default();
        let result = render_template("{% if unclosed %}", &vars);
        assert!(result.is_err());
    }

    // ── missing variable renders as empty (MiniJinja lenient mode) ──

    #[test]
    fn missing_variable_renders_empty() {
        let vars: FxHashMap<&str, &str> = FxHashMap::default();
        let result = render_template("Hello {{ missing }}!", &vars).unwrap();
        assert_eq!(result, "Hello !");
    }

    // ── validation reports both missing keys and syntax errors at once ──

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
