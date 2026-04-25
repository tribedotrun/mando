//! Structural broken-session detection.
//!
//! Replaces the former flat-substring scan with two narrow, event-aware
//! signals sourced directly from the JSONL transcript:
//!
//! 1. **Primary** — the current session's terminal `type:"result"` event
//!    has `is_error: true`. That event's presence is the broken-session
//!    verdict. Clause matching against `result.result + result.error +
//!    result.errors[]` only picks the label; a generic `cc_is_error`
//!    fallback catches every unlabelled shape.
//!
//! 2. **Secondary** — no terminal result, but the last non-`system` event
//!    is a `user/tool_result` whose `is_error:true` content matches the
//!    `SessionInterrupted` clauses (`Exit code 137` / `Request interrupted
//!    by user for tool use`). Caused by daemon SIGTERM, user interrupt, or
//!    other external termination.
//!
//! Skill prompts, user task text, assistant thinking, and routine per-tool
//! `is_error` flags on recoverable tool failures are out of scope by
//! construction — none of them live in the structural sources this module
//! inspects. See the detailed fixture matrix in
//! `crates/captain/tests/broken_session_detection.rs`.

use std::path::Path;

use crate::stream::{current_session_lines, get_stream_result};
use crate::stream_symptoms::{CcStreamSymptom, StreamSymptomMatcher};

/// Where a broken-session signal originated. Lets observability distinguish
/// "CC aborted itself" from "CLI was killed by an external signal" — two
/// operationally distinct failure modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrokenSessionOrigin {
    /// Primary path: CC wrote a terminal `result` event with `is_error: true`.
    CcReported,
    /// Secondary path: no terminal result, but the last non-system event is a
    /// `user/tool_result/is_error:true` carrying the kill signature
    /// (`Exit code 137` / `Request interrupted by user for tool use`). Caused
    /// by daemon SIGTERM, user interrupt, or other external termination.
    SessionKilled,
}

impl BrokenSessionOrigin {
    /// Stable log/metric tag. Consumed by VictoriaLogs queries and captain
    /// timeline strings; treat as part of the public contract.
    pub fn tag(self) -> &'static str {
        match self {
            Self::CcReported => "cc_reported",
            Self::SessionKilled => "session_killed",
        }
    }
}

/// A classified broken-session verdict. The typed `name` routes downstream
/// decisions; `reason` is the stable log/obs tag; `origin` distinguishes
/// CC-reported aborts from externally-killed sessions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrokenSessionMatch {
    pub name: CcStreamSymptom,
    pub reason: String,
    pub origin: BrokenSessionOrigin,
}

/// Detect a broken session via structural signals.
///
/// Returns `Some` when either the primary or secondary path fires; `None`
/// otherwise. `ImageDimensionLimit` is deliberately excluded — that
/// recoverable symptom stays on the nudge path and is surfaced by
/// [`detect_image_dimension_blocked`].
///
/// See the module-level doc comment for the full decision tree.
pub fn stream_broken_session_symptom(
    stream_path: &Path,
    symptoms: &StreamSymptomMatcher,
) -> Option<BrokenSessionMatch> {
    if let Some(result) = get_stream_result(stream_path) {
        return broken_session_from_result(&result, symptoms);
    }
    detect_session_interrupted(stream_path, symptoms)
}

/// Primary path: the current session has a terminal `result` event. When
/// `is_error:true`, build the classification corpus and pick a label. When
/// no rule matches, synthesize the generic `cc_is_error` fallback so the
/// session still routes to review.
fn broken_session_from_result(
    result: &serde_json::Value,
    symptoms: &StreamSymptomMatcher,
) -> Option<BrokenSessionMatch> {
    if result.get("is_error").and_then(|v| v.as_bool()) != Some(true) {
        return None;
    }
    let corpus = result_error_corpus(result);
    let lower = corpus.to_ascii_lowercase();
    // First matching broken-session rule wins, except SessionInterrupted
    // which is only valid on the secondary path and must never label a
    // terminal result.
    for rule in symptoms.rules() {
        if !rule.broken_session {
            continue;
        }
        if rule.name == CcStreamSymptom::SessionInterrupted {
            continue;
        }
        if rule.matches_lower(&lower) {
            return Some(BrokenSessionMatch {
                name: rule.name,
                reason: rule.reason.clone(),
                origin: BrokenSessionOrigin::CcReported,
            });
        }
    }
    // Structural signal without a specific label → generic fallback.
    // Covers mock 529s, API Error 400 Advisor, Not logged in, Request timed
    // out, content-filtering blocks, clarifier-spawn-fail, and any future
    // CC-side abort shape we haven't catalogued yet. Log which fields were
    // populated so operators can tell "CC emitted is_error with no text"
    // (empty corpus) from "we have text but no clause matched" and add a
    // new clause if needed.
    tracing::debug!(
        module = "global-claude-broken-session",
        origin = "cc_reported",
        has_result = result.get("result").is_some_and(|v| v.is_string()),
        has_error = result.get("error").is_some_and(|v| v.is_string()),
        has_errors_array = result.get("errors").is_some_and(|v| v.is_array()),
        corpus_len = corpus.len(),
        "no specific broken-session rule matched; synthesizing cc_is_error fallback"
    );
    Some(BrokenSessionMatch {
        name: CcStreamSymptom::IsError,
        reason: "cc_is_error".to_string(),
        origin: BrokenSessionOrigin::CcReported,
    })
}

/// Concatenate the string-valued fields CC uses to report a terminal error:
/// `result` (string), `error` (string), and `errors[]` (array of strings).
/// Null / non-string entries are skipped. Returns an empty string if none
/// of the fields are populated — in that case only the synthetic generic
/// fallback can fire.
fn result_error_corpus(result: &serde_json::Value) -> String {
    let mut parts: Vec<&str> = Vec::new();
    if let Some(s) = result.get("result").and_then(|v| v.as_str()) {
        parts.push(s);
    }
    if let Some(s) = result.get("error").and_then(|v| v.as_str()) {
        parts.push(s);
    }
    let errors_array: Vec<&str> = result
        .get("errors")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|e| e.as_str()).collect())
        .unwrap_or_default();
    parts.extend(errors_array);
    parts.join("\n")
}

/// Secondary path: scan the current session's events backward past all
/// `type:"system"` events (`hook_response`, `task_notification`, etc. that
/// CC appends after a SIGTERM) to the last non-system parseable event. If
/// it's a `user` event carrying a `tool_result` with `is_error:true` and
/// content matching the `SessionInterrupted` rule, return a `SessionKilled`
/// match.
fn detect_session_interrupted(
    stream_path: &Path,
    symptoms: &StreamSymptomMatcher,
) -> Option<BrokenSessionMatch> {
    let si_rule = symptoms.rule_by_name(CcStreamSymptom::SessionInterrupted)?;
    let (content, last_init_idx) = current_session_lines(stream_path)?;
    let lines: Vec<&str> = content.lines().collect();
    for line in lines[last_init_idx..].iter().rev() {
        let val: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => {
                // Most common cause: a partial write-in-progress at EOF when
                // the CC CLI is being SIGTERMed. The truncation is itself
                // evidence of a kill, so don't treat it as fatal — keep
                // scanning backward for the last fully-written event. Log
                // at debug so operators can still see the truncation if
                // they're investigating a missing classification.
                tracing::debug!(
                    module = "global-claude-broken-session",
                    stream_path = %stream_path.display(),
                    line_len = line.len(),
                    line_head = %line.chars().take(80).collect::<String>(),
                    %e,
                    "skipping unparseable line during session-interrupted scan"
                );
                continue;
            }
        };
        let event_type = val.get("type").and_then(|t| t.as_str()).unwrap_or("");
        if event_type == "system" {
            continue;
        }
        if event_type != "user" {
            // Design: only a user `tool_result` can carry the kill signature.
            // An assistant- or result-terminated stream reached the end by
            // some path other than SIGTERM — fall back to the primary path's
            // `None` verdict. Logged so investigations can distinguish this
            // from "stream has no content at all".
            tracing::debug!(
                module = "global-claude-broken-session",
                stream_path = %stream_path.display(),
                event_type,
                "secondary path: last non-system event is not `user`, bailing"
            );
            return None;
        }
        let Some(content_arr) = val.pointer("/message/content").and_then(|c| c.as_array()) else {
            // Hook-only user events and schema drift can produce a `user`
            // event without a `message.content` array. Log and bail rather
            // than silently returning None so the bail path is observable.
            tracing::debug!(
                module = "global-claude-broken-session",
                stream_path = %stream_path.display(),
                "secondary path: last user event has no message.content array, bailing"
            );
            return None;
        };
        for block in content_arr {
            if block.get("type").and_then(|t| t.as_str()) != Some("tool_result") {
                continue;
            }
            if block.get("is_error").and_then(|v| v.as_bool()) != Some(true) {
                continue;
            }
            let text = extract_tool_result_text(block.get("content"));
            if si_rule.matches(&text) {
                return Some(BrokenSessionMatch {
                    name: CcStreamSymptom::SessionInterrupted,
                    reason: si_rule.reason.clone(),
                    origin: BrokenSessionOrigin::SessionKilled,
                });
            }
        }
        return None;
    }
    None
}

/// Detect an image-dimension-limit error on the nudge path.
///
/// CC emits the dimension-limit error as a `user/tool_result` content
/// string with `is_error:true`. This walks the current session's events
/// from the tail, looks at the last such event, and matches its text
/// against the configured `ImageDimensionLimit` rule. The rule is the only
/// `broken_session: false` entry and stays on the nudge path.
///
/// Returns `true` when the last user `tool_result` carries the dimension
/// error; `false` otherwise.
pub fn detect_image_dimension_blocked(stream_path: &Path, symptoms: &StreamSymptomMatcher) -> bool {
    let Some(rule) = symptoms.rule_by_name(CcStreamSymptom::ImageDimensionLimit) else {
        return false;
    };
    let Some((content, last_init_idx)) = current_session_lines(stream_path) else {
        return false;
    };
    let lines: Vec<&str> = content.lines().collect();
    // Scan backward past non-user events (system/assistant/result) to find
    // the most recent user event. Inspect ONLY that event — the dimension
    // nudge must reflect the current state, not an earlier recovered-from
    // block. Re-firing after a successful retry would leak a stale signal.
    for line in lines[last_init_idx..].iter().rev() {
        let val: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => {
                tracing::debug!(
                    module = "global-claude-broken-session",
                    stream_path = %stream_path.display(),
                    line_len = line.len(),
                    %e,
                    "skipping unparseable line during image-dimension scan"
                );
                continue;
            }
        };
        if val.get("type").and_then(|t| t.as_str()) != Some("user") {
            continue;
        }
        let Some(arr) = val.pointer("/message/content").and_then(|c| c.as_array()) else {
            // Most recent user event has no content array (hook-only shape
            // or schema drift). Bail — we commit to the *last* user event
            // by design; earlier events do not count.
            tracing::debug!(
                module = "global-claude-broken-session",
                stream_path = %stream_path.display(),
                "image-dimension: last user event has no message.content array, bailing"
            );
            return false;
        };
        for block in arr {
            if block.get("type").and_then(|t| t.as_str()) != Some("tool_result") {
                continue;
            }
            if block.get("is_error").and_then(|v| v.as_bool()) != Some(true) {
                continue;
            }
            let text = extract_tool_result_text(block.get("content"));
            if rule.matches(&text) {
                return true;
            }
        }
        // Inspected the last user event; it did not carry a matching
        // dimension error. Stop rather than walking further back, which
        // would resurrect stale errors the worker already recovered from.
        return false;
    }
    false
}

/// Flatten a tool_result's `content` field into a single string. CC writes
/// this either as a plain string or as a list of `{type, text}` blocks
/// (matching Anthropic's content-block schema). Non-text blocks are skipped.
fn extract_tool_result_text(content: Option<&serde_json::Value>) -> String {
    match content {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Array(arr)) => arr
            .iter()
            .filter_map(|b| b.get("text").and_then(|t| t.as_str()).map(String::from))
            .collect::<Vec<_>>()
            .join("\n"),
        _ => String::new(),
    }
}
