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
}
