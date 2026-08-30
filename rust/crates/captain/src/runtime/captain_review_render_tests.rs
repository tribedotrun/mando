//! Render-phase assertions for the `captain_review` prompt.
//!
//! The prompt was rewritten to one shared verdict list (the enforced JSON
//! schema, not the prose, gates which verdicts a trigger may return) with no
//! evidence checklists (typed gates fire deterministically before review).
//! These tests hold the Rust-side variable contract to that template: exactly
//! ten variables, each reaching the section that consumes it.

use crate::runtime::captain_review_helpers::review_template_vars;

/// The exact key set both spawn paths insert, read off the shared builder
/// itself rather than a hand-maintained list.
fn builder_var_keys() -> std::collections::BTreeSet<String> {
    let item = crate::Task::new("var contract");
    review_template_vars(
        &item,
        "gates_pass",
        crate::ReviewTrigger::GatesPass,
        String::new(),
        String::new(),
        String::new(),
        String::new(),
    )
    .keys()
    .map(|k| k.to_string())
    .collect()
}

/// Every variable the template consumes, with a benign value. Individual
/// tests override the one they are exercising.
fn base_vars() -> rustc_hash::FxHashMap<&'static str, &'static str> {
    let mut vars: rustc_hash::FxHashMap<&str, &str> = rustc_hash::FxHashMap::default();
    vars.insert("trigger", "gates_pass");
    vars.insert("problem_statement", "");
    vars.insert("worker_contexts", "");
    vars.insert("work_summary", "");
    vars.insert("knowledge_base", "");
    vars.insert("evidence_images", "");
    vars.insert("is_no_pr", "");
    vars.insert("is_bug_fix", "");
    vars.insert("is_ci_failure", "");
    vars.insert("workpad_path", "/data/plans/1/workpad.md");
    vars
}

fn template() -> String {
    settings::CaptainWorkflow::compiled_default()
        .prompts
        .get("captain_review")
        .expect("captain_review template exists")
        .clone()
}

fn render(vars: &rustc_hash::FxHashMap<&str, &str>) -> String {
    let workflow = settings::CaptainWorkflow::compiled_default();
    settings::render_prompt("captain_review", &workflow.prompts, vars)
        .expect("captain_review renders with the declared variable set")
}

/// Identifiers the template actually references, from `{{ name }}` and
/// `{% if name %}`. Hand-rolled rather than regex so the test carries no new
/// dependency; the template uses only these two forms.
fn template_variables(src: &str) -> std::collections::BTreeSet<String> {
    let mut found = std::collections::BTreeSet::new();
    let bytes: Vec<char> = src.chars().collect();
    let mut i = 0;
    while i + 1 < bytes.len() {
        let opener: Option<&str> = match (bytes[i], bytes[i + 1]) {
            ('{', '{') => Some("expr"),
            ('{', '%') => Some("tag"),
            _ => None,
        };
        let Some(kind) = opener else {
            i += 1;
            continue;
        };
        let close = if kind == "expr" {
            ('}', '}')
        } else {
            ('%', '}')
        };
        let mut j = i + 2;
        let mut body = String::new();
        while j + 1 < bytes.len() && !(bytes[j] == close.0 && bytes[j + 1] == close.1) {
            body.push(bytes[j]);
            j += 1;
        }
        let body = body.trim();
        if kind == "expr" {
            if body.chars().all(|c| c.is_alphanumeric() || c == '_') && !body.is_empty() {
                found.insert(body.to_string());
            }
        } else if let Some(cond) = body.strip_prefix("if ") {
            let cond = cond.trim();
            if cond.chars().all(|c| c.is_alphanumeric() || c == '_') && !cond.is_empty() {
                found.insert(cond.to_string());
            }
        }
        i = j + 2;
    }
    found
}

// ── The variable contract (work item A) ──

#[test]
fn declared_vars_exactly_match_the_template() {
    let inserted = builder_var_keys();
    let referenced = template_variables(&template());

    let missing: Vec<_> = referenced.difference(&inserted).collect();
    assert!(
        missing.is_empty(),
        "template references variables the spawn paths never insert: {missing:?}"
    );
    let unused: Vec<_> = inserted.difference(&referenced).collect();
    assert!(
        unused.is_empty(),
        "spawn paths insert variables the template never uses: {unused:?}"
    );
}

#[test]
fn base_vars_mirror_the_builder() {
    // These render tests drive the template through `base_vars()`. If it
    // drifts from the real insert set, every assertion below stops testing
    // what production actually renders.
    let from_base: std::collections::BTreeSet<String> =
        base_vars().keys().map(|k| k.to_string()).collect();
    assert_eq!(from_base, builder_var_keys());
}

#[test]
fn is_ci_failure_tracks_the_parsed_trigger() {
    let item = crate::Task::new("ci flag");
    for trigger in crate::ALL_REVIEW_TRIGGERS {
        let vars = review_template_vars(
            &item,
            trigger.as_str(),
            trigger,
            String::new(),
            String::new(),
            String::new(),
            String::new(),
        );
        let expected = if trigger == crate::ReviewTrigger::CiFailure {
            "true"
        } else {
            ""
        };
        assert_eq!(
            vars["is_ci_failure"],
            expected,
            "trigger {}",
            trigger.as_str()
        );
    }
}

#[test]
fn workpad_path_is_the_data_dir_workpad_for_the_task() {
    let mut item = crate::Task::new("workpad path");
    item.id = 4242;
    let vars = review_template_vars(
        &item,
        "gates_pass",
        crate::ReviewTrigger::GatesPass,
        String::new(),
        String::new(),
        String::new(),
        String::new(),
    );
    let expected = global_infra::paths::data_dir()
        .join("plans")
        .join("4242")
        .join("workpad.md")
        .display()
        .to_string();
    assert_eq!(vars["workpad_path"], expected);
}

#[test]
fn dropped_variables_are_gone_from_the_template() {
    // These fed the old checklist-style prompt. Each one cost a per-review DB
    // read or file read; none survives the rewrite.
    let src = template();
    for dead in [
        "title",
        "item_id",
        "evidence_files",
        "intervention_count",
        "has_screenshot",
        "has_recording",
        "has_before_fix",
        "has_after_fix",
        "has_cannot_reproduce",
        "has_before_screenshot",
        "has_after_screenshot",
        "has_after_recording",
    ] {
        assert!(
            !src.contains(&format!("{{{{ {dead} }}}}")),
            "template still reads dead variable {dead}"
        );
    }
    assert!(
        !src.contains("| default("),
        "dead `| default(...)` companion filters must be gone"
    );
}

// ── Variables reach their sections ──

#[test]
fn trigger_renders_in_the_header() {
    let mut vars = base_vars();
    vars.insert("trigger", "gates_pass");
    assert!(render(&vars).contains("Trigger: **gates_pass**"));
}

#[test]
fn every_review_trigger_renders_its_own_name() {
    // `ReviewTrigger::Retry` is produced in code (`unwrap_or(Retry)` in
    // tick_review and the dashboard action path) but was absent from the old
    // string allowlist that fed the prompt. The header now reads the trigger
    // string directly, so every variant renders.
    for trigger in crate::ALL_REVIEW_TRIGGERS {
        let name = trigger.as_str();
        let mut vars = base_vars();
        vars.insert("trigger", name);
        let rendered = render(&vars);
        assert!(
            rendered.contains(&format!("Trigger: **{name}**")),
            "trigger {name} must render in the header"
        );
    }
}

#[test]
fn retry_trigger_renders() {
    let mut vars = base_vars();
    vars.insert("trigger", "retry");
    let rendered = render(&vars);
    assert!(rendered.contains("Trigger: **retry**"));
    // A retry review is a normal-tier review: the full verdict list applies.
    assert!(rendered.contains("**ship**"));
    assert!(rendered.contains("**nudge**"));
}

#[test]
fn problem_statement_renders_under_a_task_heading() {
    let mut vars = base_vars();
    vars.insert("problem_statement", "Fix the scrolling bug in the feed");
    let rendered = render(&vars);
    assert!(rendered.contains("## Task"));
    assert!(rendered.contains("Fix the scrolling bug in the feed"));
}

#[test]
fn empty_problem_statement_drops_the_task_heading() {
    assert!(!render(&base_vars()).contains("## Task"));
}

#[test]
fn worker_contexts_always_render() {
    let mut vars = base_vars();
    vars.insert("worker_contexts", "### Worker: mando-worker-0");
    let rendered = render(&vars);
    assert!(rendered.contains("## Worker Context"));
    assert!(rendered.contains("### Worker: mando-worker-0"));
}

#[test]
fn work_summary_renders_when_present_and_is_dropped_when_empty() {
    let mut vars = base_vars();
    vars.insert("work_summary", "Replaced the poller with an SSE patch");
    let rendered = render(&vars);
    assert!(rendered.contains("## Work Summary"));
    assert!(rendered.contains("Replaced the poller with an SSE patch"));

    assert!(
        !render(&base_vars()).contains("## Work Summary"),
        "an empty work summary must not render a bare heading"
    );
}

#[test]
fn knowledge_base_renders_when_present_and_is_dropped_when_empty() {
    let mut vars = base_vars();
    vars.insert("knowledge_base", "Prefer respawn over escalate on 429s");
    let rendered = render(&vars);
    assert!(rendered.contains("## Knowledge Base"));
    assert!(rendered.contains("Prefer respawn over escalate on 429s"));
    assert!(!render(&base_vars()).contains("## Knowledge Base"));
}

#[test]
fn evidence_section_lists_files_and_points_at_the_deck() {
    let mut vars = base_vars();
    vars.insert("evidence_images", "- /data/evidence/before.png (before)");
    let rendered = render(&vars);
    assert!(rendered.contains("## Evidence"));
    assert!(rendered.contains("- /data/evidence/before.png (before)"));
    assert!(
        rendered.contains(".ai/evidence/deck.html"),
        "the reviewer must be pointed at the worker's evidence deck"
    );
    assert!(
        rendered.contains("ffmpeg"),
        "video sampling instruction must survive"
    );
}

#[test]
fn evidence_section_drops_entirely_when_there_are_no_files() {
    assert!(!render(&base_vars()).contains("## Evidence"));
}

// ── Verdict list is no longer trigger-gated ──

#[test]
fn all_six_verdicts_render_for_every_trigger() {
    // The prompt states one list and defers the per-trigger narrowing to the
    // enforced schema (see captain_review_schema_tests). The old five
    // trigger-specific enumerations are gone.
    for trigger in [
        "gates_pass",
        "broken_session",
        "spawn_fail",
        "clarifier_fail",
    ] {
        let mut vars = base_vars();
        vars.insert("trigger", trigger);
        let rendered = render(&vars);
        for verdict in [
            "**ship**",
            "**nudge**",
            "**respawn**",
            "**reset_budget**",
            "**retry_clarifier**",
            "**escalate**",
        ] {
            assert!(
                rendered.contains(verdict),
                "{trigger} should render the shared verdict list entry {verdict}"
            );
        }
        assert!(
            rendered.contains("The enforced schema limits which of these this trigger allows."),
            "{trigger} must say the schema does the gating"
        );
    }
}

#[test]
fn old_trigger_gated_prose_is_gone() {
    let rendered = render(&base_vars());
    for stale in [
        "Available Verdicts",
        "Escalation is not available at this tier",
        "Bug-fix evidence",
        "has_before_screenshot",
        "has_after_recording",
        "Confidence Grading",
    ] {
        assert!(
            !rendered.contains(stale),
            "stale prose {stale:?} still renders"
        );
    }
}

// ── Confidence ──

#[test]
fn confidence_section_offers_high_and_mid_only() {
    let rendered = render(&base_vars());
    assert!(rendered.contains("## Confidence (ship only)"));
    assert!(rendered.contains("`high` auto-merges with no human look; `mid` stops for one."));
    assert!(
        !rendered.contains("`low`"),
        "the prompt must not offer a `low` grade the schema rejects"
    );
    assert!(rendered.contains("confidence_reason"));
}

#[test]
fn confidence_cites_the_workpad_instead_of_a_diff_on_no_pr_tasks() {
    let mut vars = base_vars();
    vars.insert("is_no_pr", "true");
    vars.insert("workpad_path", "/data/plans/42/workpad.md");
    let rendered = render(&vars);
    assert!(
        rendered.contains("no diff on this task — cite the workpad at `/data/plans/42/workpad.md`"),
        "the no-PR confidence branch must inject the real workpad path"
    );
}

// ── Special cases ──

#[test]
fn no_pr_case_renders_the_workpad_path() {
    let mut vars = base_vars();
    vars.insert("is_no_pr", "true");
    vars.insert("workpad_path", "/data/plans/42/workpad.md");
    let rendered = render(&vars);
    assert!(rendered.contains("No-PR task"));
    assert!(
        rendered.contains("/data/plans/42/workpad.md"),
        "the no-PR reviewer must be given the data-dir workpad path, which \
         lives outside the review cwd"
    );
    assert!(rendered.contains("screenshots are not required"));
}

#[test]
fn no_pr_case_and_its_workpad_path_drop_for_pr_tasks() {
    let rendered = render(&base_vars());
    assert!(!rendered.contains("No-PR task"));
    assert!(
        !rendered.contains("/data/plans/1/workpad.md"),
        "workpad_path is inserted for every review but must only surface on no-PR tasks"
    );
}

#[test]
fn bug_fix_case_renders_only_for_bug_fixes() {
    let mut vars = base_vars();
    vars.insert("is_bug_fix", "true");
    let rendered = render(&vars);
    assert!(rendered.contains("Bug fix:"));
    assert!(rendered.contains("cannot-reproduce"));
    assert!(!render(&base_vars()).contains("Bug fix:"));
}

#[test]
fn ci_failure_case_renders_only_for_ci_failures() {
    let mut vars = base_vars();
    vars.insert("trigger", "ci_failure");
    vars.insert("is_ci_failure", "true");
    let rendered = render(&vars);
    assert!(rendered.contains("CI failure:"));
    assert!(rendered.contains("gh pr checks"));

    let mut other = base_vars();
    other.insert("trigger", "timeout");
    assert!(!render(&other).contains("CI failure:"));
}

// ── Deterministic-gate framing ──

#[test]
fn prompt_states_typed_gates_already_fired() {
    // Rust owns the kind/freshness/motion gates now (see
    // service::worker_context + service::deterministic). The prompt must say
    // so, or the reviewer re-litigates checks it has no data for.
    let rendered = render(&base_vars());
    assert!(rendered.contains("enforced deterministically before this review"));
    assert!(rendered.contains("missing-gate nudges have already fired"));
}

#[test]
fn prompt_forbids_delegating_the_review() {
    let rendered = render(&base_vars());
    assert!(rendered.contains("Do not spawn subagents or delegate to review skills"));
}
