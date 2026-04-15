//! Auto-merge triage gate logic — pure, testable state derivation and
//! spawn-decision rules. Kept separate from the orchestration module
//! (`auto_merge_triage.rs`) so unit tests can exercise this logic without
//! touching tokio / the DB / CC sessions.

use serde::{Deserialize, Serialize};
use tracing::warn;

use mando_types::timeline::{TimelineEvent, TimelineEventType};

/// Structured output from the triage agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct TriageResult {
    pub confidence: String,
    pub reason: String,
}

/// JSON Schema for the triage structured output.
pub(crate) fn triage_json_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "confidence": {
                "type": "string",
                "enum": ["high", "mid", "low"],
                "description": "Confidence that this task can be merged without human review"
            },
            "reason": {
                "type": "string",
                "description": "Brief explanation of the confidence assessment"
            }
        },
        "required": ["confidence", "reason"]
    })
}

/// Outcome of polling a completed triage CC session. `None` from
/// `check_triage_outcome` signals the session is still pending.
pub(crate) enum TriageOutcome {
    /// CC produced a valid structured verdict.
    Verdict(TriageResult),
    /// CC finished but with an error / malformed / empty output. Carries the
    /// human-readable error text for the timeline/notification.
    Failed(String),
}

/// Derived state of the triage cycle for a single task, computed from the
/// timeline events. Used to decide whether to spawn, wait for backoff, or
/// emit exhaustion.
#[derive(Debug, Clone, Default)]
pub(crate) struct TriageGateState {
    /// Most recent successful triage timestamp (RFC 3339). None if never triaged successfully.
    pub last_success_at: Option<String>,
    /// Number of failed attempts in the current cycle.
    pub failures_in_cycle: u32,
    /// Most recent failure timestamp in the current cycle.
    pub last_failure_at: Option<String>,
    /// True if an `AutoMergeTriageExhausted` event is already emitted for
    /// the current cycle.
    pub exhausted_in_cycle: bool,
    /// True if a cycle is open (either no prior success OR a human reopen
    /// occurred after the last success/exhaustion).
    pub cycle_open: bool,
}

/// Derive the gate state from a chronologically-ordered list of
/// {auto_merge_triage*, human_reopen, rework_requested} events.
pub(crate) fn derive_gate_state(events: &[TimelineEvent]) -> TriageGateState {
    let mut state = TriageGateState {
        last_success_at: None,
        failures_in_cycle: 0,
        last_failure_at: None,
        exhausted_in_cycle: false,
        cycle_open: true, // never triaged → first cycle is open
    };
    for event in events {
        match event.event_type {
            TimelineEventType::AutoMergeTriage => {
                // Verdict — close the cycle, reset counters.
                state.last_success_at = Some(event.timestamp.clone());
                state.failures_in_cycle = 0;
                state.last_failure_at = None;
                state.exhausted_in_cycle = false;
                state.cycle_open = false;
            }
            TimelineEventType::HumanReopen | TimelineEventType::ReworkRequested => {
                // Human action — open a fresh cycle with a full retry budget.
                state.cycle_open = true;
                state.failures_in_cycle = 0;
                state.last_failure_at = None;
                state.exhausted_in_cycle = false;
            }
            TimelineEventType::AutoMergeTriageFailed => {
                state.failures_in_cycle = state.failures_in_cycle.saturating_add(1);
                state.last_failure_at = Some(event.timestamp.clone());
            }
            TimelineEventType::AutoMergeTriageExhausted => {
                state.exhausted_in_cycle = true;
                state.cycle_open = false;
            }
            _ => {}
        }
    }
    state
}

/// Decision for the current tick given the gate state and workflow config.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SpawnDecision {
    /// Spawn a new triage attempt.
    Spawn { attempt: u32 },
    /// Skip this tick for the given reason (reason is for diagnostic logging).
    Skip(&'static str),
    /// Attempts are exhausted — emit an exhaustion event instead of spawning.
    EmitExhausted,
}

/// Decide whether to spawn a triage attempt, wait for backoff, or emit exhaustion.
///
/// `now_rfc3339` is the current timestamp; extracted as a parameter so unit
/// tests can inject deterministic time.
pub(crate) fn decide_spawn(
    state: &TriageGateState,
    max_attempts: u32,
    backoff_s: &[u64],
    now_rfc3339: &str,
) -> SpawnDecision {
    if !state.cycle_open {
        return SpawnDecision::Skip("no cycle open — waiting for human reopen");
    }
    if state.exhausted_in_cycle {
        return SpawnDecision::Skip("cycle already exhausted");
    }
    if state.failures_in_cycle >= max_attempts {
        return SpawnDecision::EmitExhausted;
    }
    // Backoff check: require `backoff_s[failures_in_cycle - 1]` elapsed
    // since the last failure before attempting again.
    if state.failures_in_cycle > 0 {
        let backoff_idx = (state.failures_in_cycle - 1) as usize;
        let backoff = backoff_s.get(backoff_idx).copied().unwrap_or(0);
        if backoff > 0 {
            if let Some(last_fail_at) = &state.last_failure_at {
                match (
                    time::OffsetDateTime::parse(
                        last_fail_at,
                        &time::format_description::well_known::Rfc3339,
                    ),
                    time::OffsetDateTime::parse(
                        now_rfc3339,
                        &time::format_description::well_known::Rfc3339,
                    ),
                ) {
                    (Ok(last), Ok(now)) => {
                        let elapsed = (now - last).whole_seconds().max(0) as u64;
                        if elapsed < backoff {
                            return SpawnDecision::Skip("within backoff window");
                        }
                    }
                    _ => {
                        // Conservative: if either timestamp is unparseable we
                        // can't prove the backoff has elapsed, so skip rather
                        // than spawn. The next tick will retry the parse with
                        // a fresh `now`, which usually clears transient
                        // formatting glitches; persistent failures stay
                        // skipped (safer than hammering CC).
                        warn!(
                            module = "captain",
                            last_fail_at = %last_fail_at,
                            now = %now_rfc3339,
                            "failed to parse triage backoff timestamps; skipping spawn"
                        );
                        return SpawnDecision::Skip("backoff timestamp parse failed");
                    }
                }
            }
        }
    }
    SpawnDecision::Spawn {
        attempt: state.failures_in_cycle + 1,
    }
}

/// Extract a human-readable error message from a CC result payload.
///
/// CC can place error text in either the `error` field (when synthesized by
/// `mando_cc::write_error_result`) or the `result` field (when CC itself
/// reports an error like a stream idle timeout). Try both in order.
pub(crate) fn extract_cc_error_text(result: &serde_json::Value) -> String {
    if let Some(s) = result.get("error").and_then(|v| v.as_str()) {
        if !s.is_empty() {
            return s.to_string();
        }
    }
    if let Some(s) = result.get("result").and_then(|v| v.as_str()) {
        if !s.is_empty() {
            return s.to_string();
        }
    }
    "unknown error".to_string()
}

/// Look up the last error message (for exhaustion context) from the most
/// recent `AutoMergeTriageFailed` event in the current cycle.
pub(crate) fn last_failure_error(events: &[TimelineEvent]) -> Option<String> {
    events
        .iter()
        .rev()
        .find(|e| e.event_type == TimelineEventType::AutoMergeTriageFailed)
        .and_then(|e| e.data.get("error").and_then(|v| v.as_str()))
        .map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use mando_types::timeline::{TimelineEvent, TimelineEventType};

    fn ev(ty: TimelineEventType, ts: &str) -> TimelineEvent {
        TimelineEvent {
            event_type: ty,
            timestamp: ts.to_string(),
            actor: "t".into(),
            summary: String::new(),
            data: serde_json::Value::Null,
        }
    }

    fn failure_with_err(ts: &str, err: &str) -> TimelineEvent {
        TimelineEvent {
            event_type: TimelineEventType::AutoMergeTriageFailed,
            timestamp: ts.to_string(),
            actor: "t".into(),
            summary: String::new(),
            data: serde_json::json!({"error": err}),
        }
    }

    #[test]
    fn no_events_opens_first_cycle() {
        let state = derive_gate_state(&[]);
        assert!(state.cycle_open);
        assert_eq!(state.failures_in_cycle, 0);
        assert!(state.last_success_at.is_none());
        assert!(!state.exhausted_in_cycle);
    }

    #[test]
    fn success_closes_cycle() {
        let events = vec![ev(
            TimelineEventType::AutoMergeTriage,
            "2026-04-14T20:00:00Z",
        )];
        let state = derive_gate_state(&events);
        assert!(!state.cycle_open);
        assert_eq!(
            state.last_success_at.as_deref(),
            Some("2026-04-14T20:00:00Z")
        );
        assert_eq!(state.failures_in_cycle, 0);
    }

    #[test]
    fn human_reopen_after_success_opens_new_cycle() {
        let events = vec![
            ev(TimelineEventType::AutoMergeTriage, "2026-04-14T20:00:00Z"),
            ev(TimelineEventType::HumanReopen, "2026-04-14T21:00:00Z"),
        ];
        let state = derive_gate_state(&events);
        assert!(state.cycle_open);
        assert_eq!(state.failures_in_cycle, 0);
    }

    #[test]
    fn rework_requested_opens_new_cycle_like_human_reopen() {
        let events = vec![
            ev(TimelineEventType::AutoMergeTriage, "2026-04-14T20:00:00Z"),
            ev(TimelineEventType::ReworkRequested, "2026-04-14T21:00:00Z"),
        ];
        let state = derive_gate_state(&events);
        assert!(state.cycle_open);
    }

    #[test]
    fn failures_accumulate_in_cycle() {
        let events = vec![
            ev(
                TimelineEventType::AutoMergeTriageFailed,
                "2026-04-14T20:00:00Z",
            ),
            ev(
                TimelineEventType::AutoMergeTriageFailed,
                "2026-04-14T20:05:00Z",
            ),
        ];
        let state = derive_gate_state(&events);
        assert_eq!(state.failures_in_cycle, 2);
        assert_eq!(
            state.last_failure_at.as_deref(),
            Some("2026-04-14T20:05:00Z")
        );
        assert!(state.cycle_open);
    }

    #[test]
    fn success_resets_failure_counter() {
        let events = vec![
            ev(
                TimelineEventType::AutoMergeTriageFailed,
                "2026-04-14T20:00:00Z",
            ),
            ev(TimelineEventType::AutoMergeTriage, "2026-04-14T20:05:00Z"),
        ];
        let state = derive_gate_state(&events);
        assert_eq!(state.failures_in_cycle, 0);
        assert!(state.last_failure_at.is_none());
        assert!(!state.cycle_open);
    }

    #[test]
    fn exhaustion_closes_cycle() {
        let events = vec![
            ev(
                TimelineEventType::AutoMergeTriageFailed,
                "2026-04-14T20:00:00Z",
            ),
            ev(
                TimelineEventType::AutoMergeTriageFailed,
                "2026-04-14T20:05:00Z",
            ),
            ev(
                TimelineEventType::AutoMergeTriageFailed,
                "2026-04-14T20:10:00Z",
            ),
            ev(
                TimelineEventType::AutoMergeTriageExhausted,
                "2026-04-14T20:11:00Z",
            ),
        ];
        let state = derive_gate_state(&events);
        assert_eq!(state.failures_in_cycle, 3);
        assert!(state.exhausted_in_cycle);
        assert!(!state.cycle_open);
    }

    #[test]
    fn human_reopen_resets_exhaustion() {
        let events = vec![
            ev(
                TimelineEventType::AutoMergeTriageFailed,
                "2026-04-14T20:00:00Z",
            ),
            ev(
                TimelineEventType::AutoMergeTriageExhausted,
                "2026-04-14T20:01:00Z",
            ),
            ev(TimelineEventType::HumanReopen, "2026-04-14T21:00:00Z"),
        ];
        let state = derive_gate_state(&events);
        assert_eq!(state.failures_in_cycle, 0);
        assert!(!state.exhausted_in_cycle);
        assert!(state.cycle_open);
    }

    #[test]
    fn decide_spawn_first_attempt_when_never_triaged() {
        let state = TriageGateState::default();
        let mut s = state;
        s.cycle_open = true;
        let d = decide_spawn(&s, 3, &[10, 20], "2026-04-14T20:00:00Z");
        assert_eq!(d, SpawnDecision::Spawn { attempt: 1 });
    }

    #[test]
    fn decide_spawn_skip_when_cycle_closed() {
        let s = TriageGateState {
            cycle_open: false,
            last_success_at: Some("2026-04-14T19:00:00Z".into()),
            ..TriageGateState::default()
        };
        match decide_spawn(&s, 3, &[10, 20], "2026-04-14T20:00:00Z") {
            SpawnDecision::Skip(_) => {}
            other => panic!("expected Skip, got {other:?}"),
        }
    }

    #[test]
    fn decide_spawn_within_backoff_skips() {
        let s = TriageGateState {
            cycle_open: true,
            failures_in_cycle: 1,
            last_failure_at: Some("2026-04-14T20:00:00Z".into()),
            ..TriageGateState::default()
        };
        match decide_spawn(&s, 3, &[10, 20], "2026-04-14T20:00:05Z") {
            SpawnDecision::Skip(_) => {}
            other => panic!("expected Skip (backoff), got {other:?}"),
        }
    }

    #[test]
    fn decide_spawn_after_backoff_spawns_retry() {
        let s = TriageGateState {
            cycle_open: true,
            failures_in_cycle: 1,
            last_failure_at: Some("2026-04-14T20:00:00Z".into()),
            ..TriageGateState::default()
        };
        let d = decide_spawn(&s, 3, &[10, 20], "2026-04-14T20:00:11Z");
        assert_eq!(d, SpawnDecision::Spawn { attempt: 2 });
    }

    #[test]
    fn decide_spawn_after_second_backoff_spawns_third_attempt() {
        let s = TriageGateState {
            cycle_open: true,
            failures_in_cycle: 2,
            last_failure_at: Some("2026-04-14T20:00:00Z".into()),
            ..TriageGateState::default()
        };
        let d = decide_spawn(&s, 3, &[10, 20], "2026-04-14T20:00:21Z");
        assert_eq!(d, SpawnDecision::Spawn { attempt: 3 });
    }

    #[test]
    fn decide_spawn_emit_exhausted_when_at_cap() {
        let s = TriageGateState {
            cycle_open: true,
            failures_in_cycle: 3,
            last_failure_at: Some("2026-04-14T20:00:00Z".into()),
            ..TriageGateState::default()
        };
        assert_eq!(
            decide_spawn(&s, 3, &[10, 20], "2026-04-14T20:10:00Z"),
            SpawnDecision::EmitExhausted
        );
    }

    #[test]
    fn decide_spawn_skip_when_already_exhausted() {
        let s = TriageGateState {
            cycle_open: false,
            exhausted_in_cycle: true,
            failures_in_cycle: 3,
            ..TriageGateState::default()
        };
        match decide_spawn(&s, 3, &[10, 20], "2026-04-14T20:10:00Z") {
            SpawnDecision::Skip(_) => {}
            other => panic!("expected Skip, got {other:?}"),
        }
    }

    #[test]
    fn decide_spawn_zero_backoff_allows_immediate_retry() {
        let s = TriageGateState {
            cycle_open: true,
            failures_in_cycle: 1,
            last_failure_at: Some("2026-04-14T20:00:00Z".into()),
            ..TriageGateState::default()
        };
        let d = decide_spawn(&s, 3, &[0, 0], "2026-04-14T20:00:00Z");
        assert_eq!(d, SpawnDecision::Spawn { attempt: 2 });
    }

    #[test]
    fn decide_spawn_skips_when_timestamp_unparseable() {
        // If `last_failure_at` (or `now`) can't be parsed, we can't prove the
        // backoff window has elapsed, so we conservatively skip rather than
        // spawn — better to wait one extra tick than hammer CC on every tick.
        let s = TriageGateState {
            cycle_open: true,
            failures_in_cycle: 1,
            last_failure_at: Some("not-a-timestamp".into()),
            ..TriageGateState::default()
        };
        match decide_spawn(&s, 3, &[10, 20], "2026-04-14T20:00:00Z") {
            SpawnDecision::Skip(_) => {}
            other => panic!("expected Skip on parse failure, got {other:?}"),
        }
    }

    #[test]
    fn extract_cc_error_prefers_error_field_then_result() {
        let v = serde_json::json!({"error": "boom"});
        assert_eq!(extract_cc_error_text(&v), "boom");
        let v = serde_json::json!({"result": "API error: timeout"});
        assert_eq!(extract_cc_error_text(&v), "API error: timeout");
        let v = serde_json::json!({"error": "", "result": "fallback"});
        assert_eq!(extract_cc_error_text(&v), "fallback");
        let v = serde_json::json!({});
        assert_eq!(extract_cc_error_text(&v), "unknown error");
    }

    #[test]
    fn last_failure_error_returns_most_recent() {
        let events = vec![
            failure_with_err("2026-04-14T20:00:00Z", "first"),
            failure_with_err("2026-04-14T20:05:00Z", "second"),
        ];
        assert_eq!(last_failure_error(&events).as_deref(), Some("second"));
    }

    #[test]
    fn last_failure_error_returns_none_when_no_failures() {
        let events = vec![ev(
            TimelineEventType::AutoMergeTriage,
            "2026-04-14T20:00:00Z",
        )];
        assert!(last_failure_error(&events).is_none());
    }
}
