//! Typed symptoms detected in CC stream output.
//!
//! The Claude CLI emits human-readable error text into its stream when certain
//! server-side conditions fire (rate limit, image dimension limit, watchdog
//! abort, etc.). Downstream callers branch on a typed enum variant; the
//! substring patterns that map each variant to stream text live in
//! `captain-workflow.yaml` under `stream_symptoms`, not in code.
//!
//! A [`StreamSymptomMatcher`] is constructed from a rule list and exposes
//! [`StreamSymptomMatcher::detect`] returning the matching rule. First rule
//! wins — order the yaml list specific-to-generic.

use serde::{Deserialize, Serialize};

/// Typed identifier for a stream symptom. Code paths branch on these names:
/// `ImageDimensionLimit` stays on the nudge path, everything else routes to
/// broken-session review. The name is the stable contract; its pattern list
/// and response metadata come from workflow config.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum CcStreamSymptom {
    /// Recoverable: worker can resize and retry. Nudge path.
    ImageDimensionLimit,
    /// CC's stream watchdog aborted after idle timeout. Broken session.
    StreamIdleTimeout,
    /// Anthropic account hit rate/usage window. Broken session.
    RateLimitAborted,
    /// CC reported a structured `is_error: true` whose text did not match a
    /// more specific rule. Broken session. Synthesized by the detector as
    /// the generic fallback for any terminal `result/is_error:true` event.
    IsError,
    /// Session exceeded the model's context window. Broken session.
    ContextLengthExceeded,
    /// Resume attempt hit the wrong cwd. Broken session.
    NoConversationFound,
    /// External kill of the CC CLI (daemon SIGTERM, user interrupt) — no
    /// terminal `result` event, but the last tool_result carries the
    /// `Exit code 137` / `Request interrupted by user for tool use` signature.
    /// Broken session. Matched via the secondary path in
    /// [`crate::stream::stream_broken_session_symptom`].
    SessionInterrupted,
}

/// One classifier rule loaded from `stream_symptoms` in captain-workflow.yaml.
///
/// Matching semantics: AND across `clauses`, OR within each clause. A clause
/// matches when any of its substrings appears in the (lowercased) text the
/// detector hands it. All clauses must match for the rule to fire. A rule
/// with a single clause is plain OR; multiple clauses compose into AND-of-OR
/// (e.g. `ImageDimensionLimit` requires both "exceeds the dimension limit"
/// and "2000px").
///
/// **Scope (post-structural rewrite).** Clauses are never run against the
/// whole JSONL stream tail anymore. The detector in
/// [`crate::stream::stream_broken_session_symptom`] feeds narrow, structurally
/// sourced text to the matcher:
///
/// - most rules see the terminal `result` event's `result` + `error` + `errors[]` fields;
/// - `SessionInterrupted` sees only the last non-system user `tool_result`'s content;
/// - `ImageDimensionLimit` sees the last user `tool_result`'s content (via
///   [`crate::stream::detect_image_dimension_blocked`]).
///
/// Skill templates, user prompts, assistant thinking, and routine per-tool
/// errors no longer reach any rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamSymptomRule {
    /// Typed variant this rule identifies — the enum name as it appears in
    /// Rust, matched on downstream for recovery-routing decisions.
    pub name: CcStreamSymptom,
    /// Stable log/timeline tag. Must not change across versions; obs queries
    /// and captain reason strings key on this value.
    pub reason: String,
    /// True when the symptom routes to broken-session review; false when it
    /// stays on the nudge path. `ImageDimensionLimit` is the only false.
    pub broken_session: bool,
    /// AND-of-OR substring clauses. Match is case-insensitive; patterns
    /// should be written lowercase in yaml.
    pub clauses: Vec<Vec<String>>,
}

/// Owns a rule list with patterns pre-lowercased. Constructed once per tick
/// or at startup and passed into the classifier.
#[derive(Debug, Clone, Default)]
pub struct StreamSymptomMatcher {
    rules: Vec<StreamSymptomRule>,
}

impl StreamSymptomMatcher {
    pub fn new(mut rules: Vec<StreamSymptomRule>) -> Self {
        for rule in &mut rules {
            for clause in &mut rule.clauses {
                for pat in clause.iter_mut() {
                    *pat = pat.to_ascii_lowercase();
                }
            }
        }
        Self { rules }
    }

    /// Find a rule by its typed variant. Returns `None` if the configured
    /// rule list omits that variant. Used by structural detector paths that
    /// need clause data for one specific symptom (e.g. `SessionInterrupted`,
    /// `ImageDimensionLimit`) without running the full matcher.
    pub fn rule_by_name(&self, name: CcStreamSymptom) -> Option<&StreamSymptomRule> {
        self.rules.iter().find(|r| r.name == name)
    }

    /// First rule (in declaration order) whose clauses all match `text`.
    /// Kept as a `cfg(test)` helper so the test suite can exercise the
    /// generic matcher behavior without producers re-implementing the
    /// rules-iter + first-match pattern. Production detectors use
    /// [`Self::rules`] + [`StreamSymptomRule::matches`] directly so they
    /// can filter by variant (skipping `SessionInterrupted` on the
    /// primary path, etc.).
    #[cfg(test)]
    pub(crate) fn detect<'a>(&'a self, text: &str) -> Option<&'a StreamSymptomRule> {
        let lower = text.to_ascii_lowercase();
        self.rules.iter().find(|rule| rule.matches_lower(&lower))
    }

    /// Total rule count — exposed for test coverage assertions.
    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }

    /// Iterate configured rules in declaration order. Structural detectors
    /// (see [`crate::stream::stream_broken_session_symptom`]) need to walk
    /// the rule list to pick a label without running the generic matcher.
    pub fn rules(&self) -> impl Iterator<Item = &StreamSymptomRule> {
        self.rules.iter()
    }
}

impl StreamSymptomRule {
    /// True iff every clause in this rule matches the given already-lowercased
    /// text. Public so structural detectors can apply a single rule to a
    /// narrow text window without running the full matcher.
    pub fn matches_lower(&self, lower_text: &str) -> bool {
        if self.clauses.is_empty() {
            return false;
        }
        self.clauses
            .iter()
            .all(|clause| clause.iter().any(|pat| lower_text.contains(pat)))
    }

    /// Convenience: lowercases `text` then calls [`Self::matches_lower`].
    pub fn matches(&self, text: &str) -> bool {
        self.matches_lower(&text.to_ascii_lowercase())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_rules() -> Vec<StreamSymptomRule> {
        vec![
            StreamSymptomRule {
                name: CcStreamSymptom::NoConversationFound,
                reason: "no_conversation_found".into(),
                broken_session: true,
                clauses: vec![vec!["No conversation found with session ID".into()]],
            },
            StreamSymptomRule {
                name: CcStreamSymptom::StreamIdleTimeout,
                reason: "stream_idle_timeout".into(),
                broken_session: true,
                clauses: vec![vec!["Stream idle timeout".into()]],
            },
            StreamSymptomRule {
                name: CcStreamSymptom::ImageDimensionLimit,
                reason: "image_dimension_blocked".into(),
                broken_session: false,
                clauses: vec![
                    vec!["exceeds the dimension limit".into()],
                    vec!["2000px".into()],
                ],
            },
            StreamSymptomRule {
                name: CcStreamSymptom::IsError,
                reason: "cc_is_error".into(),
                broken_session: true,
                clauses: vec![vec![
                    r#""is_error":true"#.into(),
                    r#""is_error": true"#.into(),
                ]],
            },
        ]
    }

    fn matcher() -> StreamSymptomMatcher {
        StreamSymptomMatcher::new(sample_rules())
    }

    #[test]
    fn detects_image_dimension_limit_requires_both_phrases() {
        let m = matcher();
        let hit = m
            .detect("API Error: image exceeds the dimension limit of 2000px × 2000px")
            .expect("match");
        assert_eq!(hit.name, CcStreamSymptom::ImageDimensionLimit);
        // Missing the 2000px clause → no match.
        assert!(m.detect("exceeds the dimension limit").is_none());
    }

    #[test]
    fn first_rule_wins_preserves_declaration_order() {
        // StreamIdleTimeout appears before IsError in the sample list, so a
        // tail carrying both markers classifies as StreamIdleTimeout.
        let tail = r#"{"is_error":true,"error":"API Error: Stream idle timeout - partial response received"}"#;
        let m = matcher();
        let hit = m.detect(tail).expect("match");
        assert_eq!(hit.name, CcStreamSymptom::StreamIdleTimeout);
    }

    #[test]
    fn case_insensitive_match() {
        let m = matcher();
        let hit = m.detect("API ERROR: STREAM IDLE TIMEOUT").expect("match");
        assert_eq!(hit.name, CcStreamSymptom::StreamIdleTimeout);
    }

    #[test]
    fn rejects_unrelated_text() {
        assert!(matcher().detect("all good").is_none());
    }

    #[test]
    fn empty_clause_list_never_matches() {
        let matcher = StreamSymptomMatcher::new(vec![StreamSymptomRule {
            name: CcStreamSymptom::IsError,
            reason: "cc_is_error".into(),
            broken_session: true,
            clauses: vec![],
        }]);
        assert!(matcher.detect("any text at all").is_none());
    }

    #[test]
    fn default_matcher_is_empty() {
        let m = StreamSymptomMatcher::default();
        assert_eq!(m.rule_count(), 0);
        assert!(m.detect("whatever").is_none());
    }
}
