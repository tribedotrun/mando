//! Render-phase assertions for the `captain_review` prompt. The
//! `Available Verdicts` section is trigger-gated, so each test sets exactly
//! one `is_*` flag and verifies the rendered bullet list matches the branch
//! in `captain-workflow.yaml`.

fn base_vars() -> rustc_hash::FxHashMap<&'static str, &'static str> {
    let mut vars: rustc_hash::FxHashMap<&str, &str> = rustc_hash::FxHashMap::default();
    vars.insert("title", "Test");
    vars.insert("item_id", "1");
    vars.insert("worker_contexts", "");
    vars.insert("knowledge_base", "");
    vars.insert("evidence_images", "");
    vars.insert("intervention_count", "0");
    vars.insert("is_gates_pass", "");
    vars.insert("is_degraded_context", "");
    vars.insert("is_timeout", "");
    vars.insert("is_broken_session", "");
    vars.insert("is_budget_exhausted", "");
    vars.insert("is_clarifier_fail", "");
    vars.insert("is_spawn_fail", "");
    vars.insert("is_rebase_fail", "");
    vars.insert("is_ci_failure", "");
    vars.insert("is_merge_fail", "");
    vars.insert("is_repeated_nudge", "");
    vars
}

fn render(vars: &rustc_hash::FxHashMap<&str, &str>) -> String {
    let workflow = settings::CaptainWorkflow::compiled_default();
    settings::render_prompt("captain_review", &workflow.prompts, vars).unwrap()
}

#[test]
fn test_template_renders_gates_pass_verdicts() {
    let mut vars = base_vars();
    vars.insert("trigger", "gates_pass");
    vars.insert("title", "Test task");
    vars.insert("item_id", "42");
    vars.insert(
        "worker_contexts",
        "### Worker: test-worker\n- Status: in-progress",
    );
    vars.insert("intervention_count", "3");
    vars.insert("is_gates_pass", "true");

    let rendered = render(&vars);

    assert!(
        rendered.contains("test-worker"),
        "should contain worker context"
    );
    assert!(rendered.contains("**ship**"), "should have ship verdict");
    assert!(rendered.contains("**nudge**"), "should have nudge verdict");
    assert!(
        rendered.contains("**respawn**"),
        "should have respawn verdict"
    );
    assert!(
        rendered.contains("**reset_budget**"),
        "should have reset_budget verdict"
    );
    assert!(
        rendered.contains("**escalate**"),
        "gates_pass now includes escalate as an escape hatch"
    );
    assert!(
        !rendered.contains("**retry_clarifier**"),
        "no retry_clarifier for gates_pass"
    );
}

#[test]
fn test_template_renders_timeout_verdicts() {
    let mut vars = base_vars();
    vars.insert("trigger", "timeout");
    vars.insert("is_timeout", "true");

    let rendered = render(&vars);

    assert!(rendered.contains("**ship**"), "timeout should have ship");
    assert!(rendered.contains("**nudge**"), "timeout should have nudge");
    assert!(
        rendered.contains("**respawn**"),
        "timeout should have respawn"
    );
    assert!(
        rendered.contains("**reset_budget**"),
        "timeout should have reset_budget"
    );
    assert!(
        rendered.contains("**escalate**"),
        "timeout now includes escalate as an escape hatch"
    );
}

#[test]
fn test_template_renders_broken_session_verdicts() {
    // broken_session gets its own tier: ship if the work is already done,
    // respawn if a fresh worker can recover, escalate if respawn would hit
    // the same wall. It must not offer in-place resume actions.
    let mut vars = base_vars();
    vars.insert("trigger", "broken_session");
    vars.insert("is_broken_session", "true");

    let rendered = render(&vars);

    assert!(
        rendered.contains("**ship**"),
        "broken_session should have ship"
    );
    assert!(
        rendered.contains("**respawn**"),
        "broken_session should have respawn"
    );
    assert!(
        rendered.contains("**escalate**"),
        "broken_session should have escalate"
    );
    assert!(
        !rendered.contains("**nudge** —"),
        "broken_session must not offer nudge"
    );
    assert!(
        !rendered.contains("**reset_budget** —"),
        "broken_session must not offer reset_budget"
    );
    assert!(
        !rendered.contains("Escalation is not available at this tier"),
        "the old escalation-blocked prose must be gone"
    );
}

#[test]
fn test_template_renders_repeated_nudge_verdicts() {
    // repeated_nudge also rides the else tier. Same escape-hatch rationale.
    let mut vars = base_vars();
    vars.insert("trigger", "repeated_nudge");
    vars.insert("is_repeated_nudge", "true");

    let rendered = render(&vars);

    assert!(
        rendered.contains("**escalate**"),
        "repeated_nudge should have escalate"
    );
    assert!(
        !rendered.contains("Escalation is not available at this tier"),
        "repeated_nudge no longer blocks escalate"
    );
}

#[test]
fn test_template_renders_spawn_fail_verdicts() {
    let mut vars = base_vars();
    vars.insert("trigger", "spawn_fail");
    vars.insert("is_spawn_fail", "true");

    let rendered = render(&vars);

    // Available-Verdicts is trigger-gated. For spawn_fail the bullet list must
    // offer respawn + escalate only. Match each branch's unique bullet copy so
    // mentions of ship/nudge/reset_budget elsewhere in shared sections
    // (confidence grading, evidence guidance, etc.) don't produce false
    // positives.
    assert!(
        rendered.contains(
            "**respawn** — Start a fresh worktree and try again; \
             transient spawn failures often clear on retry."
        ),
        "spawn_fail must render its respawn bullet"
    );
    assert!(
        rendered.contains(
            "**escalate** — Spawn continues to fail after respawn attempts; \
             surface the underlying error to a human."
        ),
        "spawn_fail must render its escalate bullet"
    );
    assert!(
        !rendered.contains("**ship** — Work is complete, evidence type matches"),
        "spawn_fail must NOT render the default-branch ship bullet"
    );
    assert!(
        !rendered.contains("**ship** — Work is actually complete despite the budget warning"),
        "spawn_fail must NOT render the budget-exhausted ship bullet"
    );
    assert!(
        !rendered.contains("**reset_budget** —"),
        "spawn_fail must NOT render any reset_budget bullet"
    );
    assert!(
        !rendered.contains("**retry_clarifier** —"),
        "spawn_fail must NOT render retry_clarifier"
    );
}
