//! Rule 0.5 STREAM SYMPTOM (broken-session) coverage.
//!
//! Each broken-session symptom must route straight to CaptainReview with
//! `reason="broken_session"`, bypassing the mtime-based ACTIVE rule.
//! Detection is structural: only a terminal `type:"result"` event with
//! `is_error:true` (primary) or a kill-signature tool_result (secondary)
//! counts. Tests build real JSONL stream files and feed their paths to the
//! classifier.

use super::*;

fn init_line() -> &'static str {
    r#"{"type":"system","subtype":"init"}"#
}

fn result_error_line(text: &str) -> String {
    serde_json::json!({
        "type": "result",
        "subtype": "success",
        "is_error": true,
        "result": text,
    })
    .to_string()
}

#[test]
fn stream_idle_timeout_routes_to_broken_session() {
    let path = write_test_stream(
        "idle_timeout",
        &[
            init_line(),
            &result_error_line("API Error: Stream idle timeout - partial response received"),
        ],
    );
    let mut ctx = base_ctx();
    ctx.process_alive = true;
    ctx.stream_stale_s = Some(5.0);
    let a = classify_with_path(&ctx, &base_item(), None, &path);
    assert_eq!(a.action, ActionKind::CaptainReview);
    assert_eq!(a.reason.as_deref(), Some("broken_session"));
}

#[test]
fn rate_limit_routes_to_broken_session() {
    let path = write_test_stream(
        "rate_limit",
        &[
            init_line(),
            &result_error_line("You've hit your limit · resets 10am (America/Mexico_City)"),
        ],
    );
    let mut ctx = base_ctx();
    ctx.process_alive = true;
    ctx.stream_stale_s = Some(5.0);
    let a = classify_with_path(&ctx, &base_item(), None, &path);
    assert_eq!(a.action, ActionKind::CaptainReview);
    assert_eq!(a.reason.as_deref(), Some("broken_session"));
}

#[test]
fn is_error_routes_to_broken_session() {
    // Terminal result with is_error:true but no specific-rule match —
    // detector synthesizes the generic `cc_is_error` fallback and still
    // routes to broken-session review.
    let path = write_test_stream(
        "generic_is_error",
        &[
            init_line(),
            r#"{"type":"result","subtype":"error_during_execution","is_error":true,"error":"unknown"}"#,
        ],
    );
    let mut ctx = base_ctx();
    ctx.process_alive = true;
    ctx.stream_stale_s = Some(5.0);
    let a = classify_with_path(&ctx, &base_item(), None, &path);
    assert_eq!(a.action, ActionKind::CaptainReview);
    assert_eq!(a.reason.as_deref(), Some("broken_session"));
}

#[test]
fn context_length_routes_to_broken_session() {
    let path = write_test_stream(
        "context_length",
        &[init_line(), &result_error_line("Prompt is too long")],
    );
    let mut ctx = base_ctx();
    ctx.process_alive = true;
    ctx.stream_stale_s = Some(5.0);
    let a = classify_with_path(&ctx, &base_item(), None, &path);
    assert_eq!(a.action, ActionKind::CaptainReview);
    assert_eq!(a.reason.as_deref(), Some("broken_session"));
}

#[test]
fn no_conversation_found_routes_to_broken_session() {
    // Real shape: the message lives in `errors[]`, not `result` or `error`.
    let path = write_test_stream(
        "no_conversation",
        &[
            init_line(),
            r#"{"type":"result","subtype":"error_during_execution","is_error":true,"errors":["No conversation found with session ID: abc-def"]}"#,
        ],
    );
    let mut ctx = base_ctx();
    ctx.process_alive = true;
    ctx.stream_stale_s = Some(5.0);
    let a = classify_with_path(&ctx, &base_item(), None, &path);
    assert_eq!(a.action, ActionKind::CaptainReview);
    assert_eq!(a.reason.as_deref(), Some("broken_session"));
}

#[test]
fn session_interrupted_routes_to_broken_session() {
    // Secondary path: no terminal result, last non-system event is a user
    // tool_result with the kill signature (daemon SIGTERM / user interrupt).
    let path = write_test_stream(
        "session_interrupted",
        &[
            init_line(),
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"Bash","input":{}}]}}"#,
            r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","is_error":true,"content":"Exit code 137\n[Request interrupted by user for tool use]"}]}}"#,
        ],
    );
    let mut ctx = base_ctx();
    ctx.process_alive = false;
    ctx.stream_stale_s = Some(5.0);
    let a = classify_with_path(&ctx, &base_item(), None, &path);
    assert_eq!(a.action, ActionKind::CaptainReview);
    assert_eq!(a.reason.as_deref(), Some("broken_session"));
}

#[test]
fn session_interrupted_skips_trailing_system_events() {
    // CC can append `system/hook_response` or `system/task_notification` after
    // a kill. The secondary path must scan backward past these to reach the
    // real tool_result. Regression guard for the Codex reviewer finding.
    let path = write_test_stream(
        "session_interrupted_trailing_system",
        &[
            init_line(),
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"Bash","input":{}}]}}"#,
            r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","is_error":true,"content":"Exit code 137\n[Request interrupted by user for tool use]"}]}}"#,
            r#"{"type":"system","subtype":"hook_response","hook_name":"SessionEnd"}"#,
            r#"{"type":"system","subtype":"task_notification","status":"stopped"}"#,
        ],
    );
    let mut ctx = base_ctx();
    ctx.process_alive = false;
    ctx.stream_stale_s = Some(5.0);
    let a = classify_with_path(&ctx, &base_item(), None, &path);
    assert_eq!(a.action, ActionKind::CaptainReview);
    assert_eq!(a.reason.as_deref(), Some("broken_session"));
}

#[test]
fn tool_result_is_error_in_middle_does_not_route_to_broken_session() {
    // Regression guard for task 86 worker-86-2: a routine mid-session tool
    // failure (Edit-before-Read guard, failed Bash exit) must NOT trigger
    // broken-session review. The session is still working.
    let path = write_test_stream(
        "routine_tool_error",
        &[
            init_line(),
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"Edit","input":{}}]}}"#,
            r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","is_error":true,"content":"<tool_use_error>File has not been read yet. Read it first before writing to it.</tool_use_error>"}]}}"#,
            r#"{"type":"assistant","message":{"content":[{"type":"text","text":"sorry, reading first"}]}}"#,
        ],
    );
    let mut ctx = base_ctx();
    ctx.process_alive = true;
    ctx.stream_stale_s = Some(5.0);
    let a = classify_with_path(&ctx, &base_item(), None, &path);
    // Alive + fresh mtime → Rule 2 ACTIVE.
    assert_eq!(a.action, ActionKind::Skip);
    assert_eq!(a.reason.as_deref(), Some("actively working"));
}

#[test]
fn skill_template_mentioning_rate_limit_does_not_trigger() {
    // Regression guard for task 86 worker-86-2: the mando-pr-summary skill
    // template contains the literal phrase "rate limit/cache info", injected
    // as a user message. Under the old substring matcher this triggered
    // RateLimitAborted. Structural detection must ignore user/assistant
    // text entirely — only terminal result events count.
    let skill_text = "External API calls: service + rate limit/cache info";
    let path = write_test_stream(
        "skill_template_rate_limit",
        &[
            init_line(),
            &serde_json::json!({
                "type": "user",
                "message": {"role":"user","content":[{"type":"text","text": skill_text}]}
            })
            .to_string(),
            r#"{"type":"result","subtype":"success","is_error":false,"result":"done"}"#,
        ],
    );
    let mut ctx = base_ctx();
    ctx.process_alive = false;
    ctx.stream_stale_s = Some(5.0);
    let a = classify_with_path(&ctx, &base_item(), Some(true), &path);
    // Clean terminal result → not broken. No specific symptom.
    assert_ne!(a.reason.as_deref(), Some("broken_session"));
}

#[test]
fn clarifier_spawn_fail_routes_to_broken_session() {
    // Real-corpus shape: 513/616 of all is_error:true result events in
    // ~/.mando/state/cc-streams — spawn failures write a synthetic
    // `type:"result",subtype:"error"` event. Must route to review via the
    // generic fallback.
    let path = write_test_stream(
        "clarifier_spawn_fail",
        &[
            init_line(),
            r#"{"type":"result","subtype":"error","is_error":true,"error":"clarifier failed: failed to spawn claude binary at claude: No such file or directory"}"#,
        ],
    );
    let mut ctx = base_ctx();
    ctx.process_alive = false;
    ctx.stream_stale_s = Some(5.0);
    let a = classify_with_path(&ctx, &base_item(), None, &path);
    assert_eq!(a.action, ActionKind::CaptainReview);
    assert_eq!(a.reason.as_deref(), Some("broken_session"));
}

#[test]
fn mock_529_routes_to_broken_session() {
    // Sandbox mock-CC injects `API Error: 529 injected_by_cc_mock` as a
    // terminal error. Tier-2 CLI E2E specs depend on this routing through
    // broken-session review. The generic fallback catches it.
    let path = write_test_stream(
        "mock_529",
        &[
            init_line(),
            r#"{"type":"result","subtype":"error_during_execution","is_error":true,"result":"API Error: 529 injected_by_cc_mock"}"#,
        ],
    );
    let mut ctx = base_ctx();
    ctx.process_alive = false;
    ctx.stream_stale_s = Some(5.0);
    let a = classify_with_path(&ctx, &base_item(), None, &path);
    assert_eq!(a.action, ActionKind::CaptainReview);
    assert_eq!(a.reason.as_deref(), Some("broken_session"));
}

#[test]
fn image_dimension_does_not_route_to_broken_session() {
    // Recoverable: the worker can resize and retry, so it must stay on the
    // nudge path and not get respawned by broken-session review.
    //
    // Shape: CC emits the dimension error as a `user/tool_result` content
    // string with `is_error:true`. The synthesized-tail path used to read
    // `[user]` markers and miss this entirely — structural detection
    // restores the signal.
    let path = write_test_stream(
        "image_dimension",
        &[
            init_line(),
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"Read","input":{}}]}}"#,
            r#"{"type":"user","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"t1","is_error":true,"content":"API Error: image exceeds the dimension limit of 2000px × 2000px"}]}}"#,
        ],
    );
    let mut ctx = base_ctx();
    ctx.process_alive = false;
    ctx.stream_stale_s = Some(STALE_F64 + 1.0);
    let a = classify_with_path(&ctx, &base_item(), Some(false), &path);
    assert_eq!(a.action, ActionKind::Nudge);
    assert_eq!(a.reason.as_deref(), Some("image dimension blocked"));
}

#[test]
fn broken_session_symptom_wins_over_active_rule() {
    // The watchdog's injected error line refreshes stream mtime, so a wedged
    // session looks "actively working" per Rule 2. The symptom check must
    // run first and trump the staleness check.
    let path = write_test_stream(
        "win_over_active",
        &[
            init_line(),
            &result_error_line("API Error: Stream idle timeout - partial response received"),
        ],
    );
    let mut ctx = base_ctx();
    ctx.process_alive = true;
    ctx.stream_stale_s = Some(1.0);
    let a = classify_with_path(&ctx, &base_item(), None, &path);
    assert_ne!(
        a.action,
        ActionKind::Skip,
        "fresh mtime must not mask a broken-session symptom"
    );
    assert_eq!(a.action, ActionKind::CaptainReview);
    assert_eq!(a.reason.as_deref(), Some("broken_session"));
}

#[test]
fn broken_session_symptom_wins_over_timeout_rule() {
    let path = write_test_stream(
        "win_over_timeout",
        &[
            init_line(),
            &result_error_line("API Error: Stream idle timeout - no chunks received"),
        ],
    );
    let mut ctx = base_ctx();
    ctx.process_alive = true;
    ctx.stream_stale_s = Some(1.0);
    ctx.seconds_active = 25200.0;
    let a = classify_with_path(&ctx, &base_item(), None, &path);
    assert_eq!(a.action, ActionKind::CaptainReview);
    assert_eq!(a.reason.as_deref(), Some("broken_session"));
}

#[test]
fn budget_exhausted_still_wins_over_broken_session() {
    // Budget exhaustion is the terminal backstop — even a broken-session
    // symptom must route through budget_exhausted so the verdict tier picks
    // up the right trigger (which, unlike broken_session, allows Escalate
    // in the current workflow).
    let path = write_test_stream(
        "budget_beats_broken",
        &[
            init_line(),
            &result_error_line("API Error: Stream idle timeout - partial response received"),
        ],
    );
    let mut ctx = base_ctx();
    ctx.intervention_count = MAX_INT as i64;
    ctx.process_alive = true;
    ctx.stream_stale_s = Some(1.0);
    let a = classify_with_path(&ctx, &base_item(), None, &path);
    assert_eq!(a.action, ActionKind::CaptainReview);
    assert_eq!(a.reason.as_deref(), Some("budget_exhausted"));
}
