//! Template rendering for workflow prompts, nudges, and initial prompts.
//!
//! Extracted from `workflow.rs` to keep file sizes manageable.
//! Uses MiniJinja for Jinja2-style template rendering with a cached
//! environment keyed by template-map content signature.

use std::collections::HashMap;
use std::sync::{LazyLock, RwLock};

use minijinja::Environment;
use rustc_hash::FxHashMap;
use serde_json::{Map as JsonMap, Number as JsonNumber, Value as JsonValue};

// ── Template rendering ───────────────────────────────────────────────────────

/// Render a template string with MiniJinja.
///
/// `vars` is an `FxHashMap` (rustc-hash) -- the template-var hot path uses this
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
    env.set_trim_blocks(true);
    env.set_lstrip_blocks(true);
    env.set_keep_trailing_newline(true);
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
    env.set_trim_blocks(true);
    env.set_lstrip_blocks(true);
    env.set_keep_trailing_newline(true);
    env.add_template("template", template)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

fn render_template_value_map(
    template: &str,
    vars: &JsonMap<String, JsonValue>,
) -> Result<String, String> {
    let mut env = Environment::new();
    env.set_trim_blocks(true);
    env.set_lstrip_blocks(true);
    env.set_keep_trailing_newline(true);
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
    fn template_values_not_reparsed() {
        let mut vars: FxHashMap<&str, &str> = FxHashMap::default();
        vars.insert("content", "Use {{ env.VAR }} here");
        let result = render_template("Content: {{ content }}", &vars).unwrap();
        assert_eq!(result, "Content: Use {{ env.VAR }} here");
    }

    #[test]
    fn render_template_returns_err_on_bad_syntax() {
        let vars: FxHashMap<&str, &str> = FxHashMap::default();
        let result = render_template("{% if unclosed %}", &vars);
        assert!(result.is_err());
    }

    #[test]
    fn missing_variable_renders_empty() {
        let vars: FxHashMap<&str, &str> = FxHashMap::default();
        let result = render_template("Hello {{ missing }}!", &vars).unwrap();
        assert_eq!(result, "Hello !");
    }

    // ── coerce_template_scalar edge cases ───────────────────────────

    #[test]
    fn coerce_true_false_to_bool() {
        assert_eq!(coerce_template_scalar("true"), JsonValue::Bool(true));
        assert_eq!(coerce_template_scalar("false"), JsonValue::Bool(false));
    }

    #[test]
    fn coerce_case_sensitive_not_bool() {
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

    // ── trim_blocks / lstrip_blocks behavior ──────────────────────────

    #[test]
    fn trim_blocks_strips_tag_trailing_newline() {
        let tmpl = "before\n{% if show %}\nvisible\n{% endif %}\nafter";
        let mut vars: FxHashMap<&str, &str> = FxHashMap::default();
        vars.insert("show", "true");
        let result = render_template(tmpl, &vars).unwrap();
        assert_eq!(result, "before\nvisible\nafter");
    }

    #[test]
    fn trim_blocks_false_condition_no_blank_lines() {
        let tmpl = "before\n{% if show %}\nvisible\n{% endif %}\nafter";
        let vars: FxHashMap<&str, &str> = FxHashMap::default();
        let result = render_template(tmpl, &vars).unwrap();
        assert_eq!(result, "before\nafter");
    }

    #[test]
    fn lstrip_blocks_strips_leading_whitespace() {
        let tmpl = "before\n    {% if show %}\nvisible\n    {% endif %}\nafter";
        let mut vars: FxHashMap<&str, &str> = FxHashMap::default();
        vars.insert("show", "true");
        let result = render_template(tmpl, &vars).unwrap();
        assert_eq!(result, "before\nvisible\nafter");
    }

    // ── No triple-blank-lines in real workflow templates ───────────────

    fn assert_no_triple_blanks(rendered: &str, context: &str) {
        for (i, window) in rendered
            .split('\n')
            .collect::<Vec<_>>()
            .windows(3)
            .enumerate()
        {
            let all_blank = window.iter().all(|l| l.trim().is_empty());
            assert!(
                !all_blank,
                "{context}: found 3+ consecutive blank lines at line {i}"
            );
        }
    }

    fn captain_prompts() -> HashMap<String, String> {
        let wf: crate::CaptainWorkflow =
            serde_yaml::from_str(include_str!("../assets/captain-workflow.yaml")).unwrap();
        wf.prompts
    }

    fn scout_prompts() -> HashMap<String, String> {
        let wf: crate::workflow_scout::ScoutWorkflow =
            serde_yaml::from_str(include_str!("../assets/scout-workflow.yaml")).unwrap();
        wf.prompts
    }

    #[test]
    fn worker_initial_no_triple_blanks_with_optionals() {
        let prompts = captain_prompts();
        let mut vars: FxHashMap<&str, &str> = FxHashMap::default();
        vars.insert("title", "Test task");
        vars.insert("context", "Some context");
        vars.insert("id", "1");
        vars.insert("branch", "mando/test-1");
        vars.insert("no_pr", "false");
        vars.insert("original_prompt", "fix the bug");
        vars.insert("worker_preamble", "run sandbox");
        vars.insert("check_command", "`mando-dev check`");
        vars.insert("workpad_path", "/tmp/plans/1/workpad.md");

        let rendered = render_prompt("worker_initial", &prompts, &vars).unwrap();
        assert_no_triple_blanks(&rendered, "worker_initial (all optionals set)");
    }

    #[test]
    fn worker_initial_no_triple_blanks_without_optionals() {
        let prompts = captain_prompts();
        let mut vars: FxHashMap<&str, &str> = FxHashMap::default();
        vars.insert("title", "Test task");
        vars.insert("context", "Some context");
        vars.insert("id", "1");
        vars.insert("branch", "mando/test-1");
        vars.insert("no_pr", "false");
        vars.insert("original_prompt", "");
        vars.insert("worker_preamble", "");
        vars.insert("check_command", "`mando-dev check`");
        vars.insert("workpad_path", "/tmp/plans/1/workpad.md");

        let rendered = render_prompt("worker_initial", &prompts, &vars).unwrap();
        assert_no_triple_blanks(&rendered, "worker_initial (no optionals)");

        // Verify include junctions preserve blank line before headers
        assert!(
            rendered.contains("what was accomplished\n\n## Instructions"),
            "missing blank line before ## Instructions (include junction)"
        );
        assert!(
            rendered.contains("escalate to human.\n\n## Branch"),
            "missing blank line before ## Branch (include junction)"
        );
    }

    #[test]
    fn worker_initial_no_triple_blanks_no_pr() {
        let prompts = captain_prompts();
        let mut vars: FxHashMap<&str, &str> = FxHashMap::default();
        vars.insert("title", "Research task");
        vars.insert("context", "Some context");
        vars.insert("id", "2");
        vars.insert("branch", "mando/research-2");
        vars.insert("no_pr", "true");
        vars.insert("original_prompt", "");
        vars.insert("worker_preamble", "");
        vars.insert("check_command", "`mando-dev check`");
        vars.insert("workpad_path", "/tmp/plans/2/workpad.md");

        let rendered = render_prompt("worker_initial", &prompts, &vars).unwrap();
        assert_no_triple_blanks(&rendered, "worker_initial (no_pr)");
    }

    #[test]
    fn captain_review_no_triple_blanks() {
        let prompts = captain_prompts();
        let mut vars: FxHashMap<&str, &str> = FxHashMap::default();
        vars.insert("trigger", "gates_pass");
        vars.insert("worker_contexts", "Worker did some work");
        vars.insert("knowledge_base", "");
        vars.insert("evidence_images", "");
        vars.insert("is_gates_pass", "true");
        vars.insert("is_degraded_context", "false");
        vars.insert("is_timeout", "false");
        vars.insert("is_broken_session", "false");
        vars.insert("is_repeated_nudge", "false");
        vars.insert("is_rebase_fail", "false");
        vars.insert("is_ci_failure", "false");
        vars.insert("is_merge_fail", "false");
        vars.insert("is_budget_exhausted", "false");
        vars.insert("is_clarifier_fail", "false");
        vars.insert("intervention_count", "0");

        let rendered = render_prompt("captain_review", &prompts, &vars).unwrap();
        assert_no_triple_blanks(&rendered, "captain_review (gates_pass)");
    }

    #[test]
    fn captain_review_no_triple_blanks_all_false() {
        let prompts = captain_prompts();
        let mut vars: FxHashMap<&str, &str> = FxHashMap::default();
        vars.insert("trigger", "timeout");
        vars.insert("worker_contexts", "Worker did some work");
        vars.insert("knowledge_base", "");
        vars.insert("evidence_images", "");
        vars.insert("is_gates_pass", "false");
        vars.insert("is_degraded_context", "false");
        vars.insert("is_timeout", "true");
        vars.insert("is_broken_session", "false");
        vars.insert("is_repeated_nudge", "false");
        vars.insert("is_rebase_fail", "false");
        vars.insert("is_ci_failure", "false");
        vars.insert("is_merge_fail", "false");
        vars.insert("is_budget_exhausted", "false");
        vars.insert("is_clarifier_fail", "false");
        vars.insert("intervention_count", "0");

        let rendered = render_prompt("captain_review", &prompts, &vars).unwrap();
        assert_no_triple_blanks(&rendered, "captain_review (only timeout)");
    }

    #[test]
    fn scout_process_no_triple_blanks() {
        let prompts = scout_prompts();
        let mut vars: FxHashMap<&str, &str> = FxHashMap::default();
        vars.insert("user_context", "AI engineer building tools");
        vars.insert("interests_high", "AI, Rust");
        vars.insert("interests_low", "Sports");
        vars.insert("url_type", "blog");
        vars.insert("url", "https://example.com");
        vars.insert("content_path", "/tmp/content.md");

        let rendered = render_prompt("process", &prompts, &vars).unwrap();
        assert_no_triple_blanks(&rendered, "scout process (with context)");
    }

    #[test]
    fn scout_process_no_triple_blanks_no_context() {
        let prompts = scout_prompts();
        let mut vars: FxHashMap<&str, &str> = FxHashMap::default();
        vars.insert("user_context", "");
        vars.insert("interests_high", "AI, Rust");
        vars.insert("interests_low", "Sports");
        vars.insert("url_type", "blog");
        vars.insert("url", "https://example.com");
        vars.insert("content_path", "/tmp/content.md");

        let rendered = render_prompt("process", &prompts, &vars).unwrap();
        assert_no_triple_blanks(&rendered, "scout process (no context)");
    }

    #[test]
    fn scout_qa_no_triple_blanks() {
        let prompts = scout_prompts();
        let mut vars: FxHashMap<&str, &str> = FxHashMap::default();
        vars.insert("user_context", "AI engineer");
        vars.insert("summary", "Article about Rust");
        vars.insert("raw_content_note", "/tmp/raw.md");
        vars.insert("question", "What is Rust?");

        let rendered = render_prompt("qa", &prompts, &vars).unwrap();
        assert_no_triple_blanks(&rendered, "scout qa (with raw_content)");
    }

    #[test]
    fn evidence_rendered_output_no_blank_artifacts() {
        let prompts = captain_prompts();

        // Render worker_initial with all optionals empty (worst case for blank lines)
        let mut vars: FxHashMap<&str, &str> = FxHashMap::default();
        vars.insert("title", "Test task");
        vars.insert("context", "Some context");
        vars.insert("id", "1");
        vars.insert("branch", "mando/test-1");
        vars.insert("no_pr", "false");
        vars.insert("original_prompt", "");
        vars.insert("worker_preamble", "");
        vars.insert("check_command", "`mando-dev check`");
        vars.insert("workpad_path", "/tmp/plans/1/workpad.md");

        let rendered = render_prompt("worker_initial", &prompts, &vars).unwrap();

        // Print to stdout for evidence capture
        eprintln!("--- worker_initial (no optionals, no_pr=false) ---");
        for (i, line) in rendered.lines().enumerate() {
            let marker = if line.trim().is_empty() { "·" } else { " " };
            eprintln!("{:3}{marker}| {line}", i + 1);
        }
        eprintln!("--- end ---");

        // Count max consecutive blank lines
        let mut max_consecutive = 0u32;
        let mut current = 0u32;
        for line in rendered.lines() {
            if line.trim().is_empty() {
                current += 1;
                max_consecutive = max_consecutive.max(current);
            } else {
                current = 0;
            }
        }
        eprintln!("Max consecutive blank lines: {max_consecutive}");
        assert!(
            max_consecutive <= 2,
            "found {max_consecutive} consecutive blank lines"
        );
    }

    #[test]
    fn todo_parse_preserves_line_breaks() {
        let prompts = captain_prompts();
        let mut vars: FxHashMap<&str, &str> = FxHashMap::default();
        vars.insert("project", "mando");
        vars.insert("line_count", "5");
        vars.insert("text", "fix the bug\nadd tests");

        let rendered = render_prompt("todo_parse", &prompts, &vars).unwrap();
        assert!(
            rendered.contains("project."),
            "todo_parse: Project context missing"
        );
        assert!(
            rendered.contains("## Rules"),
            "todo_parse: Rules section missing"
        );
    }

    #[test]
    fn todo_parse_no_project_starts_with_rules() {
        let prompts = captain_prompts();
        let mut vars: FxHashMap<&str, &str> = FxHashMap::default();
        vars.insert("project", "");
        vars.insert("line_count", "5");
        vars.insert("text", "fix the bug");

        let rendered = render_prompt("todo_parse", &prompts, &vars).unwrap();
        assert!(
            !rendered.contains("Project context"),
            "todo_parse: Project context shown when project is empty"
        );
        assert!(
            rendered.contains("## Rules"),
            "todo_parse: Rules section missing"
        );
    }

    #[test]
    fn scout_qa_inline_endif_preserves_content() {
        let prompts = scout_prompts();
        let mut vars: FxHashMap<&str, &str> = FxHashMap::default();
        vars.insert("user_context", "AI engineer");
        vars.insert("summary", "Article about Rust");
        vars.insert("raw_content_note", "/tmp/raw.md");
        vars.insert("question", "What is Rust?");

        let rendered = render_prompt("qa", &prompts, &vars).unwrap();
        // The inline {% endif %}- pattern should keep both bullets on separate lines
        assert!(
            rendered.contains("original content.\n"),
            "scout qa: Read tool instruction missing trailing newline"
        );
        assert!(
            rendered.contains("- Be concise"),
            "scout qa: 'Be concise' bullet missing"
        );
    }

    #[test]
    fn scout_qa_no_triple_blanks_no_raw() {
        let prompts = scout_prompts();
        let mut vars: FxHashMap<&str, &str> = FxHashMap::default();
        vars.insert("user_context", "");
        vars.insert("summary", "Article about Rust");
        vars.insert("raw_content_note", "");
        vars.insert("question", "What is Rust?");

        let rendered = render_prompt("qa", &prompts, &vars).unwrap();
        assert_no_triple_blanks(&rendered, "scout qa (no context, no raw)");
    }
}
