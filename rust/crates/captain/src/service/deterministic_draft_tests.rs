//! Draft PR gate tests split out to keep deterministic_tests.rs under the Rust file-length limit.
#![cfg(test)]

use super::*;

#[test]
fn draft_pr_blocks_gates_pass_with_specific_nudge() {
    let mut ctx = base_ctx();
    ctx.pr_is_draft = true;
    let a = classify(&ctx, &base_item(), Some(true));
    assert_eq!(a.action, ActionKind::Nudge);
    assert_eq!(a.reason.as_deref(), Some("PR is still draft"));
    assert!(
        a.message.as_deref().unwrap_or("").contains("still a draft"),
        "got: {:?}",
        a.message
    );
}

#[test]
fn draft_pr_appears_in_fallback_gate_diagnosis() {
    let mut ctx = base_ctx();
    ctx.pr_is_draft = true;
    let a = classify(&ctx, &base_item(), None);
    assert_eq!(a.action, ActionKind::Nudge);
    let reason = a.reason.as_deref().unwrap_or("");
    assert!(reason.contains("PR is still draft"), "got: {reason}");
}
