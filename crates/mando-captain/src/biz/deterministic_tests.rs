//! Tests for deterministic classifier.

use super::*;

fn test_nudges() -> HashMap<String, String> {
    mando_config::workflow::CaptainWorkflow::compiled_default().nudges
}

const TIMEOUT: f64 = 21600.0;
const STALE: f64 = 1200.0;
const MAX_INT: u32 = 50;

fn base_ctx() -> WorkerContext {
    WorkerContext {
        session_name: "mando-worker-0".into(),
        item_title: "Test".into(),
        status: "in-progress".into(),
        branch: Some("feat/x".into()),
        pr: Some("1".into()),
        pr_ci_status: Some("success".into()),
        pr_comments: 0,
        unresolved_threads: 0,
        unreplied_threads: 0,
        unaddressed_issue_comments: 0,
        pr_body: "## PR Summary\n<!-- pr-summary-head: abc -->\n### After\n![fix](https://example.com/fix.png)".into(),
        changed_files: vec![],
        branch_ahead: true,
        process_alive: false,
        cpu_time_s: Some(100.0),
        prev_cpu_time_s: Some(90.0),
        stream_tail: "done".into(),
        seconds_active: 5400.0,
        intervention_count: 0,
        no_pr: false,
        reopen_seq: 0,
        has_reopen_ack: true,
        reopen_source: None,
        stream_stale_s: None,
        pr_head_sha: "abc123".into(),
        degraded: false,
        github_repo_configured: true,
    }
}

fn base_item() -> Task {
    let mut item = Task::new("Test");
    item.status = mando_types::task::ItemStatus::InProgress;
    item
}

fn classify(ctx: &WorkerContext, item: &Task, stream: Option<bool>) -> Action {
    classify_worker(
        ctx,
        Some(item),
        stream,
        false,
        &test_nudges(),
        TIMEOUT,
        STALE,
        MAX_INT,
    )
    .expect("always returns Some")
}

// ── Rule 1: TIMEOUT ──

#[test]
fn timeout_captain_review() {
    let mut ctx = base_ctx();
    ctx.process_alive = true;
    ctx.stream_stale_s = Some(STALE + 1.0);
    ctx.seconds_active = 25200.0; // 7h > 6h limit
    let a = classify(&ctx, &base_item(), None);
    assert_eq!(a.action, ActionKind::CaptainReview);
    assert_eq!(a.reason.as_deref(), Some("timeout"));
}

#[test]
fn timeout_fires_without_stream_file() {
    let mut ctx = base_ctx();
    ctx.process_alive = true;
    ctx.stream_stale_s = None; // no stream file — previously unreachable
    ctx.seconds_active = 25200.0; // 7h > 6h limit
    let a = classify(&ctx, &base_item(), None);
    assert_eq!(a.action, ActionKind::CaptainReview);
    assert_eq!(a.reason.as_deref(), Some("timeout"));
}

#[test]
fn timeout_disabled_when_zero() {
    let mut ctx = base_ctx();
    ctx.process_alive = true;
    ctx.stream_stale_s = Some(STALE + 1.0);
    ctx.seconds_active = 360000.0;
    let a = classify_worker(
        &ctx,
        Some(&base_item()),
        None,
        false,
        &test_nudges(),
        0.0,
        STALE,
        MAX_INT,
    )
    .unwrap();
    // Should NOT be CaptainReview with reason "timeout"
    assert_ne!(a.reason.as_deref(), Some("timeout"));
}

// ── Rule 2: ACTIVE ──

#[test]
fn alive_streaming_skip() {
    let mut ctx = base_ctx();
    ctx.process_alive = true;
    ctx.stream_stale_s = Some(10.0);
    let a = classify(&ctx, &base_item(), None);
    assert_eq!(a.action, ActionKind::Skip);
    assert_eq!(a.reason.as_deref(), Some("actively working"));
}

#[test]
fn alive_at_threshold_not_skip() {
    let mut ctx = base_ctx();
    ctx.process_alive = true;
    ctx.stream_stale_s = Some(STALE); // exactly at threshold, not below
    let a = classify(&ctx, &base_item(), None);
    assert_ne!(a.action, ActionKind::Skip);
}

// ── Rule 3: CC REVIEW ──

#[test]
fn gates_pass_pr_captain_review() {
    let ctx = base_ctx(); // dead, all gates pass, stream_result_clean = Some(true)
    let a = classify(&ctx, &base_item(), Some(true));
    assert_eq!(a.action, ActionKind::CaptainReview);
    assert_eq!(a.reason.as_deref(), Some("gates_pass"));
}

#[test]
fn gates_pass_nopr_captain_review() {
    let mut ctx = base_ctx();
    ctx.pr = None;
    ctx.no_pr = true;
    ctx.seconds_active = 360.0;
    ctx.stream_tail = "Research complete. Found 3 relevant patterns in the codebase.".into();
    let mut item = base_item();
    item.no_pr = true;
    let a = classify(&ctx, &item, Some(true));
    assert_eq!(a.action, ActionKind::CaptainReview);
    assert_eq!(a.reason.as_deref(), Some("gates_pass"));
}

#[test]
fn dead_stale_no_result_falls_through_to_nudge() {
    // Dead + stale > 30s + no stream result used to trigger broken_session.
    // Now it falls through to the nudge path (broad heuristic removed).
    let mut ctx = base_ctx();
    ctx.process_alive = false;
    ctx.stream_stale_s = Some(60.0);
    let a = classify(&ctx, &base_item(), None);
    assert_eq!(a.action, ActionKind::Nudge);
    let reason = a.reason.as_deref().unwrap();
    assert!(reason.starts_with("gates incomplete:"), "got: {reason}");
}

#[test]
fn budget_exhausted_captain_review() {
    let mut ctx = base_ctx();
    ctx.process_alive = false;
    ctx.intervention_count = 50;
    ctx.stream_stale_s = Some(10.0); // not broken (< 30s), but no stream result
                                     // No gates pass, not broken (stale < 30), but budget exhausted
    let a = classify(&ctx, &base_item(), None);
    assert_eq!(a.action, ActionKind::CaptainReview);
    assert_eq!(a.reason.as_deref(), Some("budget_exhausted"));
}

#[test]
fn degraded_clean_pr_routes_to_conservative_review() {
    let mut ctx = base_ctx();
    ctx.degraded = true;
    let a = classify(&ctx, &base_item(), Some(true));
    assert_eq!(a.action, ActionKind::CaptainReview);
    assert_eq!(a.reason.as_deref(), Some("degraded_context"));
}

#[test]
fn degraded_pr_does_not_fire_missing_evidence_nudge() {
    let mut ctx = base_ctx();
    ctx.degraded = true;
    ctx.pr_body.clear();
    let a = classify(&ctx, &base_item(), Some(true));
    assert_eq!(a.action, ActionKind::CaptainReview);
    assert_eq!(a.reason.as_deref(), Some("degraded_context"));
}

// ── Rule 4: NUDGE ──

#[test]
fn alive_stale_nudge() {
    let mut ctx = base_ctx();
    ctx.process_alive = true;
    ctx.stream_stale_s = Some(STALE + 100.0);
    let a = classify(&ctx, &base_item(), None);
    assert_eq!(a.action, ActionKind::Nudge);
    assert_eq!(a.reason.as_deref(), Some("you appear stuck"));
}

#[test]
fn dead_no_gates_nudge_has_diagnosis() {
    let mut ctx = base_ctx();
    ctx.process_alive = false;
    ctx.pr = None;
    ctx.branch_ahead = false;
    ctx.stream_stale_s = Some(5.0); // recently active, not broken
    let a = classify(&ctx, &base_item(), None);
    assert_eq!(a.action, ActionKind::Nudge);
    let reason = a.reason.as_deref().unwrap();
    assert!(reason.starts_with("gates incomplete:"), "got: {reason}");
    assert!(reason.contains("no clean stream result"), "got: {reason}");
    assert!(reason.contains("no PR discovered"), "got: {reason}");
    // Message should also contain the diagnosis (not empty/default).
    assert!(a.message.is_some(), "nudge message should not be empty");
    assert!(
        a.message
            .as_deref()
            .unwrap()
            .contains("Quality gates incomplete"),
        "got: {:?}",
        a.message
    );
}

#[test]
fn unresolved_threads_nudge() {
    let mut ctx = base_ctx();
    ctx.process_alive = false;
    ctx.unresolved_threads = 2;
    // Has stream result but gates fail due to threads
    let a = classify(&ctx, &base_item(), Some(true));
    assert_eq!(a.action, ActionKind::Nudge);
}

#[test]
fn missing_evidence_nudge() {
    let mut ctx = base_ctx();
    ctx.process_alive = false;
    ctx.pr_body = "## PR Summary\nJust a description".into();
    let a = classify(&ctx, &base_item(), Some(true));
    assert_eq!(a.action, ActionKind::Nudge);
    assert!(a.reason.unwrap().contains("evidence"));
}

#[test]
fn missing_diagram_nudge() {
    let mut ctx = base_ctx();
    ctx.process_alive = false;
    ctx.pr_body = "No summary\n![fix](https://example.com/fix.png)".into();
    let a = classify(&ctx, &base_item(), Some(true));
    assert_eq!(a.action, ActionKind::Nudge);
    assert!(a.reason.unwrap().contains("diagram"));
}

#[test]
fn reopen_ack_missing_nudge() {
    let mut ctx = base_ctx();
    ctx.process_alive = false;
    ctx.has_reopen_ack = false;
    ctx.reopen_seq = 1;
    ctx.reopen_source = Some("human".into());
    // Gates fail because reopen_ack missing — stream result present
    let a = classify(&ctx, &base_item(), Some(true));
    assert_eq!(a.action, ActionKind::Nudge);
    assert!(a.reason.unwrap().contains("reopen"));
}

#[test]
fn image_dimension_blocked_nudge() {
    let mut ctx = base_ctx();
    ctx.process_alive = false;
    ctx.stream_tail = "Error: image exceeds the dimension limit of 2000px".into();
    // Use Some(false) — errored stream so gates don't pass, but stream result exists
    // so missing_gate_nudge kicks in and finds image blocking.
    let a = classify(&ctx, &base_item(), Some(false));
    assert_eq!(a.action, ActionKind::Nudge);
    assert!(a.reason.unwrap().contains("image"));
}

#[test]
fn nopr_insufficient_output_nudge() {
    let mut ctx = base_ctx();
    ctx.process_alive = false;
    ctx.pr = None;
    ctx.no_pr = true;
    ctx.seconds_active = 360.0;
    ctx.stream_tail = "ok".into(); // too short
    let mut item = base_item();
    item.no_pr = true;
    let a = classify(&ctx, &item, Some(true));
    assert_eq!(a.action, ActionKind::Nudge);
    assert!(a.reason.unwrap().contains("insufficient"));
}

// ── ABR-999 regression: broken session with error result ──

#[test]
fn broken_session_with_error_result_triggers_review() {
    // ABR-999: CC crashes before init but writes an error result event.
    // stream_has_broken_session = true, stream_result_clean = Some(false).
    // Previously fell through to Rule 4 Nudge; must now trigger CaptainReview.
    let mut ctx = base_ctx();
    ctx.process_alive = false;
    ctx.stream_stale_s = Some(120.0);
    let a = classify_worker(
        &ctx,
        Some(&base_item()),
        Some(false), // error result exists
        true,        // has_broken_session = true (content, no init)
        &test_nudges(),
        TIMEOUT,
        STALE,
        MAX_INT,
    )
    .unwrap();
    assert_eq!(a.action, ActionKind::CaptainReview);
    assert_eq!(a.reason.as_deref(), Some("broken_session"));
}

// ── Missing config escalation ──

#[test]
fn missing_github_config_escalates_immediately() {
    // Project has no githubRepo → captain can't discover PRs → escalate,
    // don't nudge. This prevents the infinite nudge loop from ABR-1005.
    let mut ctx = base_ctx();
    ctx.process_alive = false;
    ctx.pr = None;
    ctx.github_repo_configured = false;
    ctx.stream_stale_s = Some(5.0);
    let a = classify(&ctx, &base_item(), Some(true));
    assert_eq!(a.action, ActionKind::CaptainReview);
    assert_eq!(a.reason.as_deref(), Some("missing_github_config"));
}

#[test]
fn missing_github_config_skipped_for_no_pr_task() {
    // no_pr tasks don't need GitHub — missing config should not escalate.
    let mut ctx = base_ctx();
    ctx.pr = None;
    ctx.no_pr = true;
    ctx.github_repo_configured = false;
    ctx.seconds_active = 360.0;
    ctx.stream_tail = "Research complete. Found 3 relevant patterns.".into();
    let mut item = base_item();
    item.no_pr = true;
    let a = classify(&ctx, &item, Some(true));
    assert_eq!(a.action, ActionKind::CaptainReview);
    assert_eq!(a.reason.as_deref(), Some("gates_pass"));
}

// ── Edge cases ──

#[test]
fn dead_recently_active_no_stream_has_diagnosis() {
    // Dead, stream recently active (< 30s), no stream result → not broken
    // Budget not exhausted → falls through to diagnostic nudge
    let mut ctx = base_ctx();
    ctx.process_alive = false;
    ctx.stream_stale_s = Some(10.0);
    let a = classify(&ctx, &base_item(), None);
    assert_eq!(a.action, ActionKind::Nudge);
    let reason = a.reason.as_deref().unwrap();
    assert!(reason.starts_with("gates incomplete:"), "got: {reason}");
    assert!(reason.contains("no clean stream result"), "got: {reason}");
}

#[test]
fn alive_no_stream_data_skip() {
    let mut ctx = base_ctx();
    ctx.process_alive = true;
    // stream_stale_s = None → just started, no stream file yet → skip
    let a = classify(&ctx, &base_item(), None);
    assert_eq!(a.action, ActionKind::Skip);
    assert_eq!(a.reason.as_deref(), Some("waiting for first output"));
}

#[test]
fn always_returns_some() {
    // Verify the contract: classify_worker never returns None.
    let ctx = base_ctx();
    let result = classify_worker(
        &ctx,
        Some(&base_item()),
        None,
        false,
        &test_nudges(),
        TIMEOUT,
        STALE,
        MAX_INT,
    );
    assert!(result.is_some());
}
