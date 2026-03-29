//! Workflow configuration — loads workflow.yaml, renders prompt templates.
//!
//! Three workflow files:
//! - **Captain** (`captain/workflow.yaml`): orchestration, prompts, nudges
//! - **Scout** (`scout/workflow.yaml`): triage, article, research, QA
//! - **Voice** (`voice/workflow.yaml`): intent parsing for voice control
//!
//! Binary ships a compiled-in default; user can override at `~/.mando/workflow.yaml`
//! (captain), `~/.mando/scout-workflow.yaml` (scout), or
//! `~/.mando/voice-workflow.yaml` (voice).

use std::collections::HashMap;
use std::path::Path;

use minijinja::Environment;
use serde::{Deserialize, Serialize};
use serde_json::{Map as JsonMap, Number as JsonNumber, Value as JsonValue};

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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AgentConfig {
    pub max_concurrent: usize,
    pub resource_limits: HashMap<String, usize>,
    pub max_interventions: u32,
    pub stale_threshold_s: f64,
    pub captain_review_timeout_s: u64,
    pub captain_merge_timeout_s: u64,
    pub max_review_retries: u32,
    pub max_clarifier_retries: u32,
    pub max_rebase_retries: u32,
    pub rebase_base_delay_s: u64,
    pub worker_timeout_s: f64,
    pub clarifier_timeout_s: u64,
    pub archive_grace_secs: u64,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            max_concurrent: 10,
            resource_limits: HashMap::new(),
            max_interventions: 50,
            stale_threshold_s: 1200.0,
            captain_review_timeout_s: 1200,
            captain_merge_timeout_s: 1800,
            max_review_retries: 3,
            max_clarifier_retries: 3,
            max_rebase_retries: 5,
            rebase_base_delay_s: 30,
            worker_timeout_s: 21600.0,
            clarifier_timeout_s: 300,
            archive_grace_secs: 604800,
        }
    }
}

// ── Embedded defaults ────────────────────────────────────────────────────────

const DEFAULT_CAPTAIN_WORKFLOW: &str = include_str!("../assets/captain-workflow.yaml");

// ── Loading ──────────────────────────────────────────────────────────────────

fn parse_captain_workflow(yaml: &str) -> CaptainWorkflow {
    serde_yaml::from_str(yaml).unwrap_or_else(|e| {
        tracing::error!("failed to parse captain workflow.yaml: {e} — using compiled-in default");
        CaptainWorkflow::compiled_default()
    })
}

fn parse_scout_workflow(yaml: &str) -> ScoutWorkflow {
    serde_yaml::from_str(yaml).unwrap_or_else(|e| {
        tracing::error!("failed to parse scout workflow.yaml: {e} — using compiled-in default");
        ScoutWorkflow::compiled_default()
    })
}

/// Load captain workflow: user override at `path` if it exists, else compiled-in default.
/// Panics if required template keys are missing (fail-fast at startup).
pub fn load_captain_workflow(override_path: &Path) -> CaptainWorkflow {
    let wf = if override_path.exists() {
        match std::fs::read_to_string(override_path) {
            Ok(contents) => {
                tracing::info!("loaded captain workflow from {}", override_path.display());
                parse_captain_workflow(&contents)
            }
            Err(e) => {
                tracing::warn!(
                    "failed to read {}: {e} — using compiled-in default",
                    override_path.display()
                );
                CaptainWorkflow::compiled_default()
            }
        }
    } else {
        CaptainWorkflow::compiled_default()
    };
    crate::workflow_validate::validate_captain_workflow(&wf);
    wf
}

/// Load scout workflow: user override at `path` if it exists, else compiled-in default.
/// Repos are injected from `config` projects that have a non-empty `scout_summary`.
/// Panics if required template keys are missing (fail-fast at startup).
pub fn load_scout_workflow(override_path: &Path, config: &crate::Config) -> ScoutWorkflow {
    let mut wf = if override_path.exists() {
        match std::fs::read_to_string(override_path) {
            Ok(contents) => {
                tracing::info!("loaded scout workflow from {}", override_path.display());
                parse_scout_workflow(&contents)
            }
            Err(e) => {
                tracing::warn!(
                    "failed to read {}: {e} — using compiled-in default",
                    override_path.display()
                );
                ScoutWorkflow::compiled_default()
            }
        }
    } else {
        ScoutWorkflow::compiled_default()
    };
    inject_config_into_workflow(&mut wf, config);
    crate::workflow_validate::validate_scout_workflow(&wf);
    wf
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
pub fn render_template(template: &str, vars: &HashMap<&str, &str>) -> Result<String, String> {
    render_template_value_map(template, &coerce_template_vars(vars))
}

/// Render a named prompt from a workflow's prompt map.
pub fn render_prompt(
    template_name: &str,
    prompts: &HashMap<String, String>,
    vars: &HashMap<&str, &str>,
) -> Result<String, String> {
    let raw = prompts
        .get(template_name)
        .ok_or_else(|| format!("unknown prompt template: {template_name:?}"))?;
    render_template_value_map(raw, &coerce_template_vars(vars))
}

/// Render a named nudge from a workflow's nudge map.
pub fn render_nudge(
    template_name: &str,
    nudges: &HashMap<String, String>,
    vars: &HashMap<&str, &str>,
) -> Result<String, String> {
    let raw = nudges
        .get(template_name)
        .ok_or_else(|| format!("unknown nudge template: {template_name:?}"))?;
    Ok(render_template_value_map(raw, &coerce_template_vars(vars))?
        .trim()
        .to_string())
}

/// Render a named initial prompt from a workflow's initial-prompt map.
pub fn render_initial_prompt(
    template_name: &str,
    prompts: &HashMap<String, String>,
    vars: &HashMap<&str, &str>,
) -> Result<String, String> {
    let raw = prompts
        .get(template_name)
        .ok_or_else(|| format!("unknown initial prompt template: {template_name:?}"))?;
    render_template_value_map(raw, &coerce_template_vars(vars))
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

fn coerce_template_vars(vars: &HashMap<&str, &str>) -> JsonMap<String, JsonValue> {
    vars.iter()
        .map(|(key, value)| ((*key).to_string(), coerce_template_scalar(value)))
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
        let mut vars = HashMap::new();
        vars.insert("name", "world");
        assert_eq!(
            render_template("Hello {{ name }}!", &vars).unwrap(),
            "Hello world!"
        );
    }

    #[test]
    fn render_if_true() {
        let mut vars = HashMap::new();
        vars.insert("show", "yes");
        let result = render_template("{% if show %}visible{% endif %}", &vars).unwrap();
        assert_eq!(result, "visible");
    }

    #[test]
    fn render_if_false() {
        let vars = HashMap::new();
        let result = render_template("{% if show %}visible{% endif %}", &vars).unwrap();
        assert_eq!(result, "");
    }

    #[test]
    fn render_if_not() {
        let vars = HashMap::new();
        let result = render_template("{% if not show %}fallback{% endif %}", &vars).unwrap();
        assert_eq!(result, "fallback");
    }

    #[test]
    fn render_nested_if() {
        let mut vars = HashMap::new();
        vars.insert("a", "1");
        vars.insert("b", "2");
        let tmpl = "{% if a %}A{% if b %}B{% endif %}{% endif %}";
        assert_eq!(render_template(tmpl, &vars).unwrap(), "AB");
    }

    #[test]
    fn render_inline_if_expression() {
        let mut vars = HashMap::new();
        vars.insert("linear_id", "ENG-42");
        let tmpl = "{{ '4' if linear_id else '3' }}";
        assert_eq!(render_template(tmpl, &vars).unwrap(), "4");
    }

    #[test]
    fn render_numeric_comparison() {
        let mut vars = HashMap::new();
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
        // interests/user_context get struct defaults even from compiled yaml
        // (serde default kicks in for `interests: {}`).
        assert!(!wf.interests.high.is_empty());
        assert!(!wf.user_context.role.is_empty());
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
        let mut vars = HashMap::new();
        vars.insert("title", "Fix the login bug");
        vars.insert("context", "Auth module is broken");
        vars.insert("branch", "mando/fix-login-1");
        vars.insert("linear_id", "ENG-42");
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
    fn render_nudge_continuation() {
        let wf = CaptainWorkflow::compiled_default();
        let mut vars = HashMap::new();
        vars.insert("nudge_count", "5");
        vars.insert("max_interventions", "50");

        let result = render_nudge("continuation_preamble", &wf.nudges, &vars);
        assert!(result.is_ok());
        let rendered = result.unwrap();
        assert!(rendered.contains("5"));
        assert!(rendered.contains("50"));
    }

    #[test]
    fn render_initial_prompt_template() {
        let wf = CaptainWorkflow::compiled_default();
        let mut vars = HashMap::new();
        vars.insert("brief_filename", "brief.md");
        vars.insert("linear_id", "ENG-42");
        vars.insert("no_pr", "false");

        let rendered = render_initial_prompt("worker", &wf.initial_prompts, &vars).unwrap();
        assert!(rendered.contains(".ai/briefs/brief.md"));
        assert!(rendered.contains("ENG-42"));
    }

    #[test]
    fn render_rebase_worker_retry_guidance() {
        let wf = CaptainWorkflow::compiled_default();
        let mut vars = HashMap::new();
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
    fn template_values_not_reparsed() {
        // Values containing {{ are NOT re-parsed as template syntax because
        // they go directly into the output buffer, not back into remaining.
        let mut vars = HashMap::new();
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
        let vars = HashMap::new();
        let result = render_template("{% if unclosed %}", &vars);
        assert!(result.is_err());
    }

    // ── missing variable renders as empty (MiniJinja lenient mode) ──

    #[test]
    fn missing_variable_renders_empty() {
        let vars = HashMap::new();
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
