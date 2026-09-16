//! Typed symptoms detected in CC stream output.
//!
//! The Claude CLI emits human-readable error text into its stream when certain
//! server-side conditions fire (rate limit, watchdog abort, context overflow,
//! etc.). Downstream callers branch on a typed enum variant; the
//! substring patterns that map each variant to stream text live in
//! `captain-workflow.yaml` under `stream_symptoms`, not in code.
//!
//! A [`StreamSymptomMatcher`] is constructed from a rule list and exposes
//! [`StreamSymptomMatcher::detect`] returning the matching rule. First rule
//! wins — order the yaml list specific-to-generic.

use serde::{Deserialize, Serialize};

/// Typed identifier for a stream symptom. Code paths branch on these names to
/// route a session to broken-session review. The name is the stable contract;
/// its pattern list and response metadata come from workflow config.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
pub enum CcStreamSymptom {
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
/// with a single clause is plain OR; multiple clauses compose into AND-of-OR.
///
/// **Scope (post-structural rewrite).** Clauses are never run against the
/// whole JSONL stream tail anymore. The detector in
/// [`crate::stream::stream_broken_session_symptom`] feeds narrow, structurally
/// sourced text to the matcher:
///
/// - most rules see the terminal `result` event's `result` + `error` + `errors[]` fields;
/// - `SessionInterrupted` sees only the last non-system user `tool_result`'s content.
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
    /// stays on the nudge path. Every rule shipped today is `true`.
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
    /// need clause data for one specific symptom (e.g. `SessionInterrupted`)
    /// without running the full matcher.
    pub fn rule_by_name(&self, name: CcStreamSymptom) -> Option<&StreamSymptomRule> {
        self.rules.iter().find(|r| r.name == name)
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
