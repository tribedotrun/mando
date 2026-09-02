//! Stable nudge-reason kinds for the repeated-nudge circuit breaker.
//!
//! `Action.reason` is a human-readable sentence, and several of the reasons
//! the deterministic classifier produces embed live data:
//! `gates incomplete: 2 unresolved thread(s); missing evidence`,
//! `PR has 3 unreplied review thread(s)`, `reopen #4 pending`. Keying the
//! breaker on that text meant those reasons were never byte-equal on two
//! consecutive ticks, so the breaker could never trip for exactly the loops
//! it exists to stop — they fell through to the much larger
//! `max_interventions` budget instead.
//!
//! The breaker keys on [`NudgeReasonKind`] — a stable discriminant derived
//! from the reason — while the formatted reason still carries the detail
//! into the nudge text and the `WorkerNudged` timeline payload.

/// Stable identity of a nudge reason, independent of any data its formatted
/// message embeds. Every reason `service::deterministic` attaches to an
/// `ActionKind::Nudge` maps to one of these; `CaptainReview` and `Skip`
/// reasons never reach the nudge path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NudgeReasonKind {
    /// `gates incomplete: <failure list>` — embeds the failing-gate list.
    GatesIncomplete,
    /// `PR has <n> unresolved review thread(s) and ...` — embeds counts.
    PrHygiene,
    /// `reopen #<n> pending` — embeds the reopen sequence number.
    ReopenAck,
    DraftPr,
    MissingWorkSummary,
    MissingEvidence,
    StaleEvidence,
    StaleWorkSummary,
    /// `missing UI evidence (screenshot + recording)` — `EvidenceGap::reason`
    /// names the capture gate a UI deck is failing.
    MissingEvidenceKind,
    /// `stale UI evidence (...) — recapture after reopen`, the same gap after
    /// a reopen invalidated the captures that exist.
    StaleEvidenceKind,
    InsufficientOutput,
    StreamStale,
    /// A reason the classifier does not currently produce. Keeps the
    /// pre-existing exact-text comparison so an unmapped future reason still
    /// counts repeats instead of silently never repeating.
    Unmapped,
}

impl NudgeReasonKind {
    /// Derive the kind from a formatted reason. The data-bearing reasons
    /// match on their fixed prefix; the rest are fixed sentences.
    pub(super) fn classify(reason: &str) -> Self {
        let reason = reason.trim();
        if reason.starts_with("gates incomplete") {
            return Self::GatesIncomplete;
        }
        if reason.starts_with("PR has ") {
            return Self::PrHygiene;
        }
        if reason.starts_with("reopen #") {
            return Self::ReopenAck;
        }
        match reason {
            "PR is still draft" => Self::DraftPr,
            "missing work summary" => Self::MissingWorkSummary,
            "missing evidence" => Self::MissingEvidence,
            "evidence stale after reopen" => Self::StaleEvidence,
            "work summary stale after reopen" => Self::StaleWorkSummary,
            "insufficient output" => Self::InsufficientOutput,
            "you appear stuck" => Self::StreamStale,
            // `EvidenceGap::reason` names the failing capture gate after the
            // verb, so match the verb rather than restating the sentences
            // here — the exact wording lives with the gate that produces it.
            _ if reason.starts_with("missing ") => Self::MissingEvidenceKind,
            _ if reason.starts_with("stale ") => Self::StaleEvidenceKind,
            _ => Self::Unmapped,
        }
    }

    /// Stable key persisted in the worker's `last_nudge_reason` health
    /// field. `Unmapped` has no key — [`breaker_key`] falls back to the raw
    /// reason text for it.
    fn as_str(self) -> Option<&'static str> {
        Some(match self {
            Self::GatesIncomplete => "gates-incomplete",
            Self::PrHygiene => "pr-hygiene",
            Self::ReopenAck => "reopen-ack",
            Self::DraftPr => "draft-pr",
            Self::MissingWorkSummary => "missing-work-summary",
            Self::MissingEvidence => "missing-evidence",
            Self::StaleEvidence => "stale-evidence",
            Self::StaleWorkSummary => "stale-work-summary",
            Self::MissingEvidenceKind => "missing-evidence-kind",
            Self::StaleEvidenceKind => "stale-evidence-kind",
            Self::InsufficientOutput => "insufficient-output",
            Self::StreamStale => "stream-stale",
            Self::Unmapped => return None,
        })
    }
}

/// The value the breaker compares against, and persists into
/// `last_nudge_reason`, for `reason`.
pub(super) fn breaker_key(reason: &str) -> &str {
    NudgeReasonKind::classify(reason).as_str().unwrap_or(reason)
}

/// One step of the repeated-nudge breaker.
pub(super) struct BreakerStep<'a> {
    /// Key to persist back into `last_nudge_reason`.
    pub key: &'a str,
    /// Consecutive same-kind nudge count including this one.
    pub consecutive: u32,
}

/// Fold `reason` into the stored breaker state. `last_key` and `prev` come
/// from the worker's health record; the returned `key` is what the nudge
/// persists back, so the read side here and the write side in
/// `nudge_health::persist_nudge_health` always compare the same value.
pub(super) fn advance<'a>(last_key: Option<&str>, reason: &'a str, prev: u32) -> BreakerStep<'a> {
    let key = breaker_key(reason);
    let consecutive = if last_key == Some(key) { prev + 1 } else { 1 };
    BreakerStep { key, consecutive }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Drive the breaker exactly the way `nudge_item` reads the health
    /// record and `persist_nudge_health` writes it back: decide from the
    /// stored key/count, then store what a delivered nudge would store.
    /// Returns the 0-based index of the nudge that trips the breaker.
    fn trips_at(reasons: &[&str], threshold: u32) -> Option<usize> {
        let mut stored_key: Option<String> = None;
        let mut stored_count: u32 = 0;
        for (i, reason) in reasons.iter().enumerate() {
            let step = advance(stored_key.as_deref(), reason, stored_count);
            if step.consecutive >= threshold {
                return Some(i);
            }
            stored_key = Some(step.key.to_string());
            stored_count = step.consecutive;
        }
        None
    }

    /// The bug this module exists to fix: `gates_incomplete` embeds the
    /// failing-gate list, so three consecutive nudges for the same stuck
    /// worker carried three different strings and the exact-text breaker
    /// never saw a repeat.
    #[test]
    fn gates_incomplete_with_different_failure_lists_trips_at_the_threshold() {
        let reasons = [
            "gates incomplete: no PR created — push your branch and open a PR",
            "gates incomplete: 2 unresolved thread(s); missing evidence",
            "gates incomplete: 5 unresolved thread(s); 1 unreplied thread(s); missing evidence",
        ];
        // Pre-fix keying would have compared the raw strings, which all differ.
        assert_ne!(reasons[0], reasons[1]);
        assert_ne!(reasons[1], reasons[2]);
        assert_eq!(
            trips_at(&reasons, 3),
            Some(2),
            "the third gates_incomplete nudge must trip the default max_repeated_nudges=3"
        );
    }

    /// `unresolved_threads` embeds a comment/thread count, the other reason
    /// the old exact-text breaker could never match twice.
    #[test]
    fn unresolved_threads_with_different_counts_trips_at_the_threshold() {
        let reasons = [
            "PR has 3 unresolved review thread(s)",
            "PR has 2 unresolved review thread(s) and 1 unreplied review thread(s)",
            "PR has 1 unaddressed issue comment(s)",
        ];
        assert_eq!(trips_at(&reasons, 3), Some(2));
    }

    /// `reopen_ack` embeds the reopen sequence number.
    #[test]
    fn reopen_ack_with_different_sequence_numbers_trips_at_the_threshold() {
        let reasons = [
            "reopen #1 pending",
            "reopen #2 pending",
            "reopen #3 pending",
        ];
        assert_eq!(trips_at(&reasons, 3), Some(2));
    }

    /// Genuinely different problems must not be collapsed into one repeat
    /// chain — a worker cycling through distinct gaps is making progress.
    #[test]
    fn genuinely_different_reasons_never_trip_the_breaker() {
        let reasons = [
            "missing evidence",
            "PR is still draft",
            "you appear stuck",
            "missing work summary",
            "gates incomplete: missing evidence",
            "insufficient output",
        ];
        assert_eq!(
            trips_at(&reasons, 3),
            None,
            "distinct reason kinds must reset the consecutive counter"
        );
    }

    /// A distinct reason mid-run resets the chain, so the breaker only fires
    /// on `max_repeated_nudges` *consecutive* same-kind nudges.
    #[test]
    fn an_intervening_different_reason_resets_the_chain() {
        let reasons = [
            "gates incomplete: missing evidence",
            "gates incomplete: 1 unresolved thread(s)",
            "PR is still draft",
            "gates incomplete: missing work summary",
            "gates incomplete: 4 unresolved thread(s)",
            "gates incomplete: 9 unresolved thread(s)",
        ];
        assert_eq!(trips_at(&reasons, 3), Some(5));
    }

    /// Every reason `service::deterministic` attaches to a nudge must map to
    /// a real kind. An `Unmapped` here means a new nudge reason was added
    /// without a kind and would silently regress to exact-text comparison.
    #[test]
    fn every_classifier_nudge_reason_maps_to_a_kind() {
        let cases = [
            (
                "gates incomplete: unknown gate failure",
                NudgeReasonKind::GatesIncomplete,
            ),
            (
                "PR has 1 unresolved review thread(s)",
                NudgeReasonKind::PrHygiene,
            ),
            ("reopen #7 pending", NudgeReasonKind::ReopenAck),
            ("PR is still draft", NudgeReasonKind::DraftPr),
            ("missing work summary", NudgeReasonKind::MissingWorkSummary),
            ("missing evidence", NudgeReasonKind::MissingEvidence),
            (
                "evidence stale after reopen",
                NudgeReasonKind::StaleEvidence,
            ),
            (
                "work summary stale after reopen",
                NudgeReasonKind::StaleWorkSummary,
            ),
            ("insufficient output", NudgeReasonKind::InsufficientOutput),
            ("you appear stuck", NudgeReasonKind::StreamStale),
            // The two sentences `EvidenceGap::reason` can produce.
            (
                "missing UI evidence (screenshot + recording)",
                NudgeReasonKind::MissingEvidenceKind,
            ),
            (
                "stale UI evidence (screenshot + recording) — recapture after reopen",
                NudgeReasonKind::StaleEvidenceKind,
            ),
        ];
        for (reason, expected) in cases {
            assert_eq!(
                NudgeReasonKind::classify(reason),
                expected,
                "reason {reason:?} must map to {expected:?}"
            );
        }
    }

    /// An unrecognised reason keeps the old exact-text behaviour rather than
    /// collapsing every stranger into one shared bucket.
    #[test]
    fn unmapped_reasons_fall_back_to_exact_text() {
        assert_eq!(
            NudgeReasonKind::classify("something new the classifier learned"),
            NudgeReasonKind::Unmapped
        );
        assert_eq!(breaker_key("something new"), "something new");
        assert_eq!(trips_at(&["a", "b", "a", "b"], 3), None);
        assert_eq!(trips_at(&["a", "a", "a"], 3), Some(2));
    }
}
