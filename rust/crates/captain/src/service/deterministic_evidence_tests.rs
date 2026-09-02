//! Evidence-gate tests split out to keep deterministic_tests.rs under the
//! Rust file-length limit.
#![cfg(test)]

use super::*;

// ── UI capture gate (deterministic pre-review routing) ──

/// A UI diff, so the screenshot + recording rule binds.
fn ui_ctx() -> WorkerContext {
    let mut ctx = base_ctx();
    ctx.process_alive = false;
    ctx.changed_files = vec!["electron/src/renderer/domains/tasks/ui/TaskCard.tsx".into()];
    ctx
}

#[test]
fn ui_missing_recording_nudges_instead_of_gates_pass() {
    let mut ctx = ui_ctx();
    ctx.has_recording = false;
    let a = classify(&ctx, &base_item(), Some(true));
    assert_eq!(
        a.action,
        ActionKind::Nudge,
        "an incomplete UI deck must not reach gates_pass"
    );
    let reason = a.reason.unwrap();
    assert!(reason.contains("UI evidence"), "got: {reason}");
    // Must reuse the existing missing_evidence template, not invent one.
    assert!(a
        .message
        .as_deref()
        .unwrap()
        .contains("mando todo evidence"));
}

#[test]
fn ui_missing_screenshot_nudges() {
    let mut ctx = ui_ctx();
    ctx.has_screenshot = false;
    let a = classify(&ctx, &base_item(), Some(true));
    assert_eq!(a.action, ActionKind::Nudge);
    assert!(a.reason.unwrap().contains("UI evidence"));
}

#[test]
fn ui_screenshot_and_recording_reach_gates_pass_without_any_kind_tag() {
    // base_ctx registers untagged captures (no before/after kinds at all);
    // that is a complete UI deck.
    let ctx = ui_ctx();
    let a = classify(&ctx, &base_item(), Some(true));
    assert_eq!(a.action, ActionKind::CaptainReview);
    assert_eq!(a.reason.as_deref(), Some("gates_pass"));
}

#[test]
fn backend_only_diff_is_not_held_to_captures() {
    let mut ctx = base_ctx();
    ctx.process_alive = false;
    ctx.changed_files = vec!["rust/crates/captain/src/service/deterministic.rs".into()];
    ctx.has_screenshot = false;
    ctx.has_recording = false;
    let a = classify(&ctx, &base_item(), Some(true));
    assert_eq!(
        a.action,
        ActionKind::CaptainReview,
        "a non-UI change must not owe screenshots or recordings"
    );
    assert_eq!(a.reason.as_deref(), Some("gates_pass"));
}

#[test]
fn bug_fix_with_fresh_captures_reaches_gates_pass() {
    // A bug fix owes the same proof as any other change: the fix working.
    // There is no before/after pairing gate.
    let mut ctx = ui_ctx();
    ctx.has_cannot_reproduce = false;
    let mut item = base_item();
    item.is_bug_fix = true;
    let a = classify(&ctx, &item, Some(true));
    assert_eq!(a.action, ActionKind::CaptainReview);
    assert_eq!(a.reason.as_deref(), Some("gates_pass"));
}

#[test]
fn bug_fix_ui_deck_without_recording_still_nudges() {
    let mut ctx = ui_ctx();
    ctx.has_recording = false;
    let mut item = base_item();
    item.is_bug_fix = true;
    let a = classify(&ctx, &item, Some(true));
    assert_eq!(a.action, ActionKind::Nudge);
    assert!(a.reason.unwrap().contains("UI evidence"));
}

#[test]
fn bug_fix_cannot_reproduce_satisfies_every_capture_gate() {
    // The write-up is the whole deliverable: even a UI diff owes no captures.
    let mut ctx = ui_ctx();
    ctx.has_screenshot = false;
    ctx.has_recording = false;
    ctx.has_cannot_reproduce = true;
    let mut item = base_item();
    item.is_bug_fix = true;
    let a = classify(&ctx, &item, Some(true));
    assert_eq!(a.action, ActionKind::CaptainReview);
    assert_eq!(a.reason.as_deref(), Some("gates_pass"));
}

#[test]
fn stale_capture_gate_routes_to_stale_evidence_nudge() {
    let mut ctx = ui_ctx();
    ctx.evidence_fresh = false;
    ctx.reopen_seq = 1;
    ctx.has_screenshot = false;
    ctx.has_recording = false;
    let a = classify(&ctx, &base_item(), Some(true));
    assert_eq!(a.action, ActionKind::Nudge);
    assert!(
        a.message
            .as_deref()
            .unwrap()
            .contains("predates the latest reopen"),
        "stale evidence must reuse the stale_evidence template"
    );
}

#[test]
fn degraded_context_still_wins_over_capture_gates() {
    // A degraded PR fetch has an empty changed_files list; the classifier
    // must route to review, not manufacture a UI evidence nudge.
    let mut ctx = ui_ctx();
    ctx.degraded = true;
    ctx.has_screenshot = false;
    ctx.has_recording = false;
    let a = classify(&ctx, &base_item(), Some(true));
    assert_eq!(a.action, ActionKind::CaptainReview);
    assert_eq!(a.reason.as_deref(), Some("degraded_context"));
}

// ── Stale evidence after reopen ──

#[test]
fn stale_evidence_blocks_gates_pass() {
    let mut ctx = base_ctx();
    // Evidence exists but is stale after reopen.
    ctx.has_evidence = true;
    ctx.evidence_fresh = false;
    ctx.reopen_seq = 1;
    let a = classify(&ctx, &base_item(), Some(true));
    assert_eq!(a.action, ActionKind::Nudge);
    assert!(
        a.reason.unwrap().contains("stale"),
        "expected stale evidence nudge"
    );
}

#[test]
fn fresh_evidence_after_reopen_passes_gates() {
    let mut ctx = base_ctx();
    // DB-backed gates: evidence and summary exist and are fresh.
    ctx.has_evidence = true;
    ctx.evidence_fresh = true;
    ctx.has_work_summary = true;
    ctx.work_summary_fresh = true;
    ctx.reopen_seq = 1;
    let a = classify(&ctx, &base_item(), Some(true));
    assert_eq!(a.action, ActionKind::CaptainReview);
    assert_eq!(a.reason.as_deref(), Some("gates_pass"));
}
