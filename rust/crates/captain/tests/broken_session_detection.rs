//! Integration tests for broken-session detection.
//!
//! Each fixture in `tests/fixtures/` represents one real or synthetic CC
//! stream shape. These tests load the fixture, run the detector against the
//! compiled-default `stream_symptoms` config, and assert the expected
//! classification — covering the worker-86 regression cases and the corpus
//! of known CC terminal-error shapes.

use std::path::{Path, PathBuf};

use global_claude::{
    detect_image_dimension_blocked, stream_broken_session_symptom, BrokenSessionOrigin,
    CcStreamSymptom, StreamSymptomMatcher,
};

fn fixture(name: &str) -> PathBuf {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    crate_dir
        .join("tests")
        .join("fixtures")
        .join("broken_session")
        .join(name)
}

fn matcher() -> StreamSymptomMatcher {
    StreamSymptomMatcher::new(settings::CaptainWorkflow::compiled_default().stream_symptoms)
}

#[test]
fn fp_worker86_2_skill_template_returns_none() {
    // Clean terminal result. Skill template contains "rate limit" but that
    // text never reaches the matcher now — structural scope is
    // result.result+error+errors[] only.
    let m =
        stream_broken_session_symptom(&fixture("fp_worker86_2_skill_template.jsonl"), &matcher());
    assert!(m.is_none(), "worker-86-2 FP regression: {m:?}");
}

#[test]
fn fp_worker86_2_tool_error_returns_none() {
    // A routine mid-session tool failure with is_error:true must not trigger.
    let m = stream_broken_session_symptom(&fixture("fp_worker86_2_tool_error.jsonl"), &matcher());
    assert!(m.is_none(), "routine tool error FP regression: {m:?}");
}

#[test]
fn killed_in_flight_fires_session_interrupted() {
    let m = stream_broken_session_symptom(&fixture("killed_in_flight.jsonl"), &matcher())
        .expect("should fire");
    assert_eq!(m.name, CcStreamSymptom::SessionInterrupted);
    assert_eq!(m.reason, "session_killed");
    assert_eq!(m.origin, BrokenSessionOrigin::SessionKilled);
}

#[test]
fn killed_in_flight_skips_trailing_system_events() {
    // CC appends hook_response / task_notification after SIGTERM. The
    // detector must scan backward past these to reach the tool_result.
    let m = stream_broken_session_symptom(
        &fixture("killed_in_flight_trailing_system.jsonl"),
        &matcher(),
    )
    .expect("should fire");
    assert_eq!(m.name, CcStreamSymptom::SessionInterrupted);
    assert_eq!(m.origin, BrokenSessionOrigin::SessionKilled);
}

#[test]
fn rate_limit_real_fires_rate_limit_aborted() {
    let m =
        stream_broken_session_symptom(&fixture("rate_limit_real.jsonl"), &matcher()).expect("fire");
    assert_eq!(m.name, CcStreamSymptom::RateLimitAborted);
    assert_eq!(m.reason, "rate_limit_aborted");
    assert_eq!(m.origin, BrokenSessionOrigin::CcReported);
}

#[test]
fn idle_timeout_real_fires_stream_idle_timeout() {
    let m = stream_broken_session_symptom(&fixture("idle_timeout_real.jsonl"), &matcher())
        .expect("fire");
    assert_eq!(m.name, CcStreamSymptom::StreamIdleTimeout);
    assert_eq!(m.reason, "stream_idle_timeout");
}

#[test]
fn no_conversation_found_via_errors_array() {
    // The message lives in `errors[]`, not `result` or `error`. The detector
    // must include errors[] in the classification corpus.
    let m =
        stream_broken_session_symptom(&fixture("no_conversation_errors_array.jsonl"), &matcher())
            .expect("fire");
    assert_eq!(m.name, CcStreamSymptom::NoConversationFound);
    assert_eq!(m.reason, "no_conversation_found");
}

#[test]
fn mock_529_falls_back_to_cc_is_error() {
    // Sandbox mock: "API Error: 529 injected_by_cc_mock". Matches the IsError
    // clause list and routes to the generic label.
    let m = stream_broken_session_symptom(&fixture("mock_529.jsonl"), &matcher()).expect("fire");
    assert_eq!(m.name, CcStreamSymptom::IsError);
    assert_eq!(m.reason, "cc_is_error");
}

#[test]
fn api400_advisor_falls_back_to_cc_is_error() {
    let m =
        stream_broken_session_symptom(&fixture("api400_advisor.jsonl"), &matcher()).expect("fire");
    assert_eq!(m.name, CcStreamSymptom::IsError);
    assert_eq!(m.reason, "cc_is_error");
}

#[test]
fn not_logged_in_falls_back_to_cc_is_error() {
    let m =
        stream_broken_session_symptom(&fixture("not_logged_in.jsonl"), &matcher()).expect("fire");
    assert_eq!(m.name, CcStreamSymptom::IsError);
    assert_eq!(m.reason, "cc_is_error");
}

#[test]
fn request_timed_out_falls_back_to_cc_is_error() {
    let m = stream_broken_session_symptom(&fixture("request_timed_out.jsonl"), &matcher())
        .expect("fire");
    assert_eq!(m.name, CcStreamSymptom::IsError);
    assert_eq!(m.reason, "cc_is_error");
}

#[test]
fn content_filtering_falls_back_to_cc_is_error() {
    let m = stream_broken_session_symptom(&fixture("content_filtering.jsonl"), &matcher())
        .expect("fire");
    assert_eq!(m.name, CcStreamSymptom::IsError);
    assert_eq!(m.reason, "cc_is_error");
}

#[test]
fn clarifier_spawn_fail_falls_back_to_cc_is_error() {
    // 513/616 real is_error:true result events in the corpus — must route.
    let m = stream_broken_session_symptom(&fixture("clarifier_spawn_fail.jsonl"), &matcher())
        .expect("fire");
    assert_eq!(m.name, CcStreamSymptom::IsError);
    assert_eq!(m.reason, "cc_is_error");
}

#[test]
fn worker_thinking_rate_limit_does_not_trigger() {
    // Assistant thinking / text mentioning "rate limit" must be ignored.
    let m = stream_broken_session_symptom(
        &fixture("worker_thinking_mentions_rate_limit.jsonl"),
        &matcher(),
    );
    assert!(m.is_none(), "assistant-text FP regression: {m:?}");
}

#[test]
fn image_dimension_detects_via_tool_result_content() {
    // The old synthesized-tail check on ctx.stream_tail stripped user text
    // to `[user]` and missed this entirely. Structural detection walks the
    // JSONL events and finds the dimension-limit tool_result.
    let hit = detect_image_dimension_blocked(&fixture("image_dimension.jsonl"), &matcher());
    assert!(
        hit,
        "dimension-limit regression — guard was dead before PR #960"
    );
}

#[test]
fn image_dimension_does_not_trigger_broken_session() {
    // The recoverable symptom stays on the nudge path: broken-session detector
    // must ignore it. Secondary path fails because the kill signature is
    // absent; primary path fails because there's no terminal result event.
    let m = stream_broken_session_symptom(&fixture("image_dimension.jsonl"), &matcher());
    assert!(
        m.is_none(),
        "image dimension must NOT route to broken-session: {m:?}"
    );
}
