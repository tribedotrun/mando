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
